//! Own the X11 CLIPBOARD selection for one paste, then restore what was there.
//!
//! `frame-voice` writes a UTF-8 string to this process's stdin and waits for the
//! `ready` line on stdout. It then presses the target app's paste shortcut.
//! After the paste the daemon sends SIGUSR1: this process re-publishes the
//! clipboard contents it saved before taking ownership and keeps serving them
//! until something else takes the selection or the linger expires.
//!
//! Only UTF-8 text is preserved; non-text clipboard contents cannot be captured
//! this way and are lost on restore.

use frame_voice::process::wait_readable;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            AtomEnum, ConnectionExt, EventMask, PropMode, SelectionNotifyEvent, WindowClass,
            SELECTION_NOTIFY_EVENT,
        },
        Event,
    },
    COPY_FROM_PARENT, CURRENT_TIME,
};

/// How long to keep serving the restored clipboard after a paste.
const LINGER: Duration = Duration::from_secs(120);
/// How long to wait for the previous owner to hand over its clipboard text.
const SAVE_TIMEOUT: Duration = Duration::from_millis(400);
const MAX_SERVE: Duration = Duration::from_secs(10);

static RESTORE: AtomicBool = AtomicBool::new(false);
extern "C" fn on_usr1(_: libc::c_int) {
    RESTORE.store(true, Ordering::SeqCst);
}

/// Ask the current CLIPBOARD owner for its text. Returns None if there is no
/// owner, it is not text, or it does not answer in time.
fn save_clipboard(
    conn: &x11rb::rust_connection::RustConnection,
    win: u32,
    clipboard: u32,
    utf8: u32,
    prop: u32,
) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    conn.convert_selection(win, clipboard, utf8, prop, CURRENT_TIME)?
        .check()?;
    conn.flush()?;
    let start = Instant::now();
    while start.elapsed() < SAVE_TIMEOUT {
        match conn.poll_for_event()? {
            Some(Event::SelectionNotify(e)) if e.selection == clipboard => {
                if e.property == x11rb::NONE {
                    return Ok(None);
                }
                let reply = conn
                    .get_property(true, win, prop, AtomEnum::ANY, 0, 1024 * 1024)?
                    .reply()?;
                return Ok(Some(reply.value));
            }
            Some(_) => {}
            None => {
                wait_readable(conn.stream(), SAVE_TIMEOUT.saturating_sub(start.elapsed()))?;
            }
        }
    }
    Ok(None)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut dictated = Vec::new();
    std::io::stdin().read_to_end(&mut dictated)?;

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let win = conn.generate_id()?;
    conn.create_window(
        COPY_FROM_PARENT as u8,
        win,
        screen.root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        COPY_FROM_PARENT,
        &Default::default(),
    )?
    .check()?;

    let intern = |name: &[u8]| -> Result<u32, Box<dyn std::error::Error>> {
        Ok(conn.intern_atom(false, name)?.reply()?.atom)
    };
    let clipboard = intern(b"CLIPBOARD")?;
    let utf8 = intern(b"UTF8_STRING")?;
    let targets = intern(b"TARGETS")?;
    let string = intern(b"STRING")?;
    let text_atom = intern(b"TEXT")?;
    let save_prop = intern(b"FV_SAVE")?;

    // Save whatever is on the clipboard before we take it over.
    let saved = save_clipboard(&conn, win, clipboard, utf8, save_prop).unwrap_or(None);

    unsafe {
        libc::signal(libc::SIGUSR1, on_usr1 as *const () as libc::sighandler_t);
    }

    conn.set_selection_owner(win, clipboard, CURRENT_TIME)?
        .check()?;
    conn.flush()?;
    // Signal readiness only after ownership is established.
    println!("ready");
    std::io::stdout().flush()?;

    let start = Instant::now();
    let mut current: &[u8] = &dictated;
    let mut restore_deadline: Option<Instant> = None;
    loop {
        if RESTORE.load(Ordering::SeqCst) && restore_deadline.is_none() {
            match &saved {
                Some(text) => {
                    current = text;
                    restore_deadline = Some(Instant::now() + LINGER);
                }
                // Nothing to restore: release the selection and leave.
                None => {
                    let _ = conn.set_selection_owner(x11rb::NONE, clipboard, CURRENT_TIME);
                    break;
                }
            }
        }
        if let Some(deadline) = restore_deadline {
            if Instant::now() >= deadline {
                break;
            }
        } else if start.elapsed() >= MAX_SERVE {
            break;
        }
        match conn.poll_for_event()? {
            Some(Event::SelectionRequest(req)) => {
                let property = if req.property == x11rb::NONE {
                    req.target
                } else {
                    req.property
                };
                let ok = match req.target {
                    t if t == targets => {
                        let data: Vec<u8> = [utf8, string, text_atom]
                            .iter()
                            .flat_map(|a| a.to_le_bytes())
                            .collect();
                        conn.change_property(
                            PropMode::REPLACE,
                            req.requestor,
                            property,
                            AtomEnum::ATOM,
                            32,
                            3,
                            &data,
                        )?
                        .check()
                        .is_ok()
                    }
                    t if t == utf8 || t == string || t == text_atom => conn
                        .change_property(
                            PropMode::REPLACE,
                            req.requestor,
                            property,
                            req.target,
                            8,
                            current.len() as u32,
                            current,
                        )?
                        .check()
                        .is_ok(),
                    _ => false,
                };
                let notify = SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: req.time,
                    requestor: req.requestor,
                    selection: req.selection,
                    target: req.target,
                    property: if ok { property } else { x11rb::NONE },
                };
                conn.send_event(false, req.requestor, EventMask::NO_EVENT, notify)?;
                conn.flush()?;
            }
            Some(Event::SelectionClear(_)) => break,
            Some(_) => {}
            None => {
                let deadline = restore_deadline.unwrap_or(start + MAX_SERVE);
                // X11 activity wakes immediately; SIGUSR1 interrupts the poll.
                // The 1s ceiling bounds the signal-before-poll race without a spin.
                let timeout = deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_secs(1));
                wait_readable(conn.stream(), timeout)?;
            }
        }
    }
    Ok(())
}
