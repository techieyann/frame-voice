use crate::{
    config::Config,
    context::Focus,
    process::{wait_readable, Process},
    speech::{self, Output},
};
use anyhow::{bail, Result};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
/// Clipboard helpers that linger to re-serve the previous clipboard. Reaped by
/// `reap_helpers` so they do not become zombies.
static HELPERS: Mutex<Vec<Process>> = Mutex::new(Vec::new());
/// Characters frame-voice has typed into the classless Desktop since the last
/// clear. Clear Backspaces exactly this much, so it only ever removes our own
/// output and never touches pre-existing text.
static IMPLANTED: Mutex<usize> = Mutex::new(0);
/// Reap clipboard helpers that have finished restoring and exited.
pub fn reap_helpers() {
    if let Ok(mut list) = HELPERS.lock() {
        list.retain_mut(|child| !matches!(child.0.try_wait(), Ok(Some(_))));
    }
}
pub fn clear_keys(class: &str) -> Option<&'static [&'static str]> {
    if class.is_empty() {
        None
    } else if class.to_ascii_lowercase().contains("konsole") {
        Some(&["29:1", "22:1", "22:0", "29:0"])
    } else {
        Some(&["29:1", "30:1", "30:0", "29:0", "111:1", "111:0"])
    }
}
/// Paste shortcut per application. Konsole (and most terminals) require
/// Ctrl+Shift+V; everything else uses the conventional Ctrl+V.
fn paste_keys(class: &str) -> Option<&'static [&'static str]> {
    if class.is_empty() {
        None
    } else if class.to_ascii_lowercase().contains("konsole") {
        Some(&["29:1", "42:1", "47:1", "47:0", "42:0", "29:0"])
    } else {
        Some(&["29:1", "47:1", "47:0", "29:0"])
    }
}
/// Backspace exactly the characters frame-voice typed into a classless target
/// (the nested Desktop). `Ctrl+A` + Delete is unsafe there because focus could
/// be a file manager; deleting our own output is bounded and safe.
fn clear_implanted(c: &Config, focus: Option<&Focus>) -> Result<()> {
    if !focus.is_some_and(|f| f.class.is_empty()) {
        return Ok(());
    }
    let count = IMPLANTED
        .lock()
        .map(|mut n| std::mem::take(&mut *n))
        .unwrap_or(0);
    if count == 0 {
        return Ok(());
    }
    eprintln!("classless clear: {count} backspaces");
    // KEY_BACKSPACE is code 14. Use the configured key delay so a long dictation
    // is not sent faster than the target app consumes keystrokes.
    let delay = c.get("YDOTOOL_KEY_DELAY", "2");
    let mut args: Vec<String> = vec!["key".into(), "-d".into(), delay.into()];
    for _ in 0..count {
        args.push("14:1".into());
        args.push("14:0".into());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_ydotool(c, &refs)
}
/// Append a separator so consecutive dictations don't run together
/// (`Hello.` + `world` -> `Hello. world`). `VOICE_TRAILING=none` disables it.
fn with_trailing(mode: &str, text: &str) -> String {
    if mode == "none" || text.chars().last().is_some_and(char::is_whitespace) {
        text.to_string()
    } else {
        format!("{text} ")
    }
}
fn socket_path(c: &Config) -> String {
    c.values.get("YDOTOOL_SOCKET").cloned().unwrap_or_else(|| {
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        let owned = format!("/run/user/{uid}/frame-voice-ydotool.sock");
        if std::path::Path::new(&owned).exists() {
            return owned;
        }
        let runtime = c.get("XDG_RUNTIME_DIR", "");
        let nested = format!("{runtime}/.ydotool_socket");
        if !runtime.is_empty() && std::path::Path::new(&nested).exists() {
            nested
        } else {
            format!("/run/user/{uid}/.ydotool_socket")
        }
    })
}
fn run_ydotool(c: &Config, args: &[&str]) -> Result<()> {
    let mut process = Process(
        Command::new(c.bin("YDOTOOL_BIN", "ydotool"))
            .args(args)
            .env("YDOTOOL_SOCKET", socket_path(c))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let start = Instant::now();
    loop {
        if let Some(status) = process.0.try_wait()? {
            if !status.success() {
                bail!("ydotool failed");
            }
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(30) {
            bail!("ydotool timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
/// Paste the whole dictation atomically: own the X11 CLIPBOARD selection with a
/// helper, then press the target app's paste shortcut. This avoids per-key
/// painting, which some compositors batch into visible clumps.
fn paste(
    c: &Config,
    text: &str,
    focus: Option<&Focus>,
    mut valid: impl FnMut() -> bool,
) -> Result<()> {
    let Some(keys) = focus.and_then(|f| paste_keys(&f.class)) else {
        bail!("cannot paste: focused app class is unknown");
    };
    let payload = with_trailing(c.get("VOICE_TRAILING", "space"), text);
    if c.paste_chunk > 0 {
        return paste_chunked(c, &payload, keys, focus, &mut valid);
    }
    let mut helper = spawn_helper(c, focus)?;
    if let Some(mut input) = helper.0.stdin.take() {
        input.write_all(payload.as_bytes())?;
    }
    // Wait for the helper to own the selection before pressing paste.
    wait_helper(&mut helper, &mut valid)?;
    if !valid() {
        bail!("focus changed before paste");
    }
    run_paste_keys(c, keys)?;
    // Give the app time to request the selection, then ask the helper to
    // re-publish the saved clipboard and keep serving it. Reaped later.
    std::thread::sleep(Duration::from_millis(400));
    // SAFETY: kill with a valid signal number and pid has no preconditions.
    unsafe {
        libc::kill(helper.0.id() as libc::pid_t, libc::SIGUSR1);
    }
    if let Ok(mut list) = HELPERS.lock() {
        list.push(helper);
    }
    Ok(())
}
fn spawn_helper(c: &Config, focus: Option<&Focus>) -> Result<Process> {
    let mut command = Command::new(c.bin("CLIPBOARD_BIN", "frame-voice-paste"));
    if let Some(focus) = focus {
        command.env("DISPLAY", &focus.display);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map(Process)
        .map_err(|e| anyhow::anyhow!("start clipboard helper: {e}"))
}
/// Read the complete readiness handshake with a deadline and focus checks.
fn wait_helper(helper: &mut Process, valid: &mut impl FnMut() -> bool) -> Result<()> {
    let mut output = helper
        .0
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("clipboard helper stdout missing"))?;
    let start = Instant::now();
    let mut line = [0; 6];
    let mut received = 0;
    while received < line.len() {
        if !valid() {
            bail!("focus changed before paste");
        }
        if start.elapsed() >= Duration::from_secs(2) {
            bail!("clipboard helper readiness timed out");
        }
        if wait_readable(&output, Duration::from_millis(50))? {
            let count = output.read(&mut line[received..])?;
            if count == 0 {
                bail!("clipboard helper exited early");
            }
            received += count;
        }
    }
    if &line != b"ready\n" {
        bail!("invalid clipboard helper readiness");
    }
    Ok(())
}
fn run_paste_keys(c: &Config, keys: &[&str]) -> Result<()> {
    let delay = c.get("YDOTOOL_KEY_DELAY", "2");
    let mut args: Vec<&str> = vec!["key", "-d", delay];
    args.extend_from_slice(keys);
    run_ydotool(c, &args)
}
/// Paste `payload` as several smaller pieces so an app does not collapse a large
/// multi-line paste into a placeholder. Newlines are reproduced with Enter.
fn paste_chunked(
    c: &Config,
    payload: &str,
    keys: &[&str],
    focus: Option<&Focus>,
    valid: &mut impl FnMut() -> bool,
) -> Result<()> {
    let delay = c.get("YDOTOOL_KEY_DELAY", "2");
    for piece in speech::chunks(payload, c.paste_chunk) {
        if !piece.text.is_empty() {
            paste_once(c, &piece.text, keys, focus, valid)?;
            if !valid() {
                bail!("focus changed during paste");
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        if piece.newline {
            run_ydotool(c, &["key", "-d", delay, "28:1", "28:0"])?;
            std::thread::sleep(Duration::from_millis(80));
        }
    }
    Ok(())
}
/// One atomic paste of `text`: own the clipboard, press paste, then stop.
fn paste_once(
    c: &Config,
    text: &str,
    keys: &[&str],
    focus: Option<&Focus>,
    valid: &mut impl FnMut() -> bool,
) -> Result<()> {
    let mut helper = spawn_helper(c, focus)?;
    if let Some(mut input) = helper.0.stdin.take() {
        input.write_all(text.as_bytes())?;
    }
    wait_helper(&mut helper, valid)?;
    if !valid() {
        bail!("focus changed before paste");
    }
    run_paste_keys(c, keys)?;
    // Let the app take the selection, then stop this helper. Chunked mode does
    // not restore the previous clipboard.
    std::thread::sleep(Duration::from_millis(120));
    drop(helper);
    Ok(())
}
pub fn send(c: &Config, output: &Output, focus: Option<&Focus>) -> Result<()> {
    send_guarded(c, output, focus, || true)
}
pub fn send_guarded(
    c: &Config,
    output: &Output,
    focus: Option<&Focus>,
    mut valid: impl FnMut() -> bool,
) -> Result<()> {
    if !valid() {
        bail!("focus changed before injection");
    }
    let classless = focus.is_some_and(|f| f.class.is_empty());
    // Clear on the classless Desktop Backspaces our own typed output instead of
    // sending Ctrl+A + Delete, which could select and delete unrelated content.
    if matches!(output, Output::Clear) && classless {
        return clear_implanted(c, focus);
    }
    if let Output::Text(text) = output {
        if !text.is_ascii() && classless {
            bail!("Unicode text needs a clipboard-compatible standalone app; nested Desktop typing cannot deliver it safely");
        }
        if c.get("VOICE_INJECT", "type") == "paste" || !text.is_ascii() {
            // A classless target is the nested Desktop: it has no known paste
            // chord and runs on its own :2/Wayland session, so the X11 clipboard
            // helper cannot serve it. Fall back to uinput typing, which follows
            // whatever holds focus (X11 or native Wayland). Paste still handles
            // every app with a known WM_CLASS.
            if !classless {
                return paste(c, text, focus, valid);
            }
            eprintln!("classless focus (Desktop): typing instead of pasting");
        }
    }
    let mut cmd = Command::new(c.bin("YDOTOOL_BIN", "ydotool"));
    let delay = c.get("YDOTOOL_KEY_DELAY", "2");
    let hold = c.get("YDOTOOL_KEY_HOLD", "2");
    // Count characters we type into a classless target so Clear can undo them.
    let mut implanted = 0usize;
    match output {
        Output::Cancel => return Ok(()),
        Output::Text(text) if text.is_empty() => return Ok(()),
        Output::Text(text) => {
            let typed = with_trailing(c.get("VOICE_TRAILING", "space"), text);
            if classless {
                implanted = typed.chars().count();
                eprintln!(
                    "classless type: {implanted} chars, trailing_space={}",
                    typed.ends_with(' ')
                );
            }
            cmd.args(["type", "-d", delay, "-H", hold, "--", &typed]);
        }
        Output::Submit => {
            cmd.args(["key", "-d", delay, "28:1", "28:0"]);
        }
        Output::Clear => {
            let Some(keys) = focus.and_then(|f| clear_keys(&f.class)) else {
                bail!("cannot safely clear: focused app class is unknown");
            };
            cmd.args(["key", "-d", delay]).args(keys);
        }
    }
    let socket = socket_path(c);
    let mut process = Process(
        cmd.env("YDOTOOL_SOCKET", socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let start = Instant::now();
    loop {
        if !valid() {
            bail!("focus changed during injection; remaining typing canceled");
        }
        if let Some(status) = process.0.try_wait()? {
            if !status.success() {
                bail!("ydotool injection failed");
            }
            if implanted > 0 {
                if let Ok(mut n) = IMPLANTED.lock() {
                    *n += implanted;
                }
            }
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(30) {
            bail!("ydotool injection timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clear_is_context_aware() {
        assert!(clear_keys("").is_none());
        assert_eq!(clear_keys("konsole/Konsole").unwrap()[1], "22:1");
        assert_eq!(clear_keys("firefox").unwrap().last(), Some(&"111:0"));
    }
    #[test]
    fn paste_shortcut_is_context_aware() {
        assert!(paste_keys("").is_none());
        // Konsole needs Ctrl+Shift+V.
        assert_eq!(paste_keys("konsole/Konsole").unwrap().len(), 6);
        assert_eq!(paste_keys("firefox").unwrap().len(), 4);
    }
    #[test]
    fn trailing_separator_avoids_doubling() {
        assert_eq!(with_trailing("space", "Hello."), "Hello. ");
        assert_eq!(with_trailing("space", "Hello. "), "Hello. ");
        assert_eq!(with_trailing("none", "Hello."), "Hello.");
        // Unknown modes fall back to a space rather than silently concatenating.
        assert_eq!(with_trailing("", "x"), "x ");
    }
}
