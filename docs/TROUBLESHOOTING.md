# Troubleshooting

Start in Desktop and focus an app's text field. A new Firefox tab's address bar
is a useful first test.

## No microphone badge or tone

1. Check the tray: a white microphone means running; a white crossed-out
   microphone means disabled. Choose **Start / resume dictation** if needed.
2. Touch the joystick cap briefly, release, then touch it again and hold.
   A plain hold does not start recording.
3. Wait for the growing **blue headset badge** and start tone before speaking.
   If tones are disabled in General, the badge still indicates readiness.
4. Try **Restart dictation** in the tray, then repeat in a focused text field.

Frame Voice does not dictate into Steam menus or VR games.

## Groq produces no text

Check internet access and the key under Transcription → Groq. The
[API-key page](https://console.groq.com/keys) lets you create a key. Check Groq's
usage/limit notifications and **View recent logs** for failed requests.
Quiet or uncertain recordings can cancel instead of inserting text. Try a clear
sentence in a quieter room. Environmental sounds can still be misidentified.

## Local setup fails

Check free storage and the internet connection needed for the first download.
Setup verifies downloads and retains the current transcription path if
preparation fails. Reopen Configure and try saving again.
Delete unused models in Transcription to recover storage; first switch and save
another path to unlock deletion of an active model.

## Clear leaves text or removes the wrong text

Default Clear is **right X** or **left D-pad Left**, after text appears. Release
the button to clear. Updates preserve existing button choices; use Controllers
to switch an older B binding to X.

Start with an empty field and use a fresh dictation after an update, settings
save, or service restart. Desktop tracks text inserted by the running process;
restarting resets that history. Cursor movement and manual edits can invalidate
that count. If using B, see [B's Desktop space](SETTINGS.md#bs-desktop-space).

## Accented or non-Latin text does not appear

Use a standalone app launched directly from Steam. Typing inside Desktop
currently supports ASCII; supported standalone apps use clipboard paste for
other scripts.

## Update check cannot find a release

Check internet access and try again. **Open releases** shows the release page.
The automatic check reads public stable releases; a private repository or an
unpublished release will not appear. Experimental versions are not selected.

## Diagnostics

First use **View recent logs** in the tray. If you need to inspect services,
open Konsole and run:

```sh
~/.local/bin/frame-voice --check
systemctl --user status frame-voice frame-voice-input frame-voice-mixer frame-voice-tray-session
journalctl --user -u frame-voice -n 50 --no-pager
```

The four services handle dictation, keyboard input, microphone setup, and the
Desktop tray. `--check` validates settings and compiled support; it does not test
your API key, internet connection, or headset health.

When reporting a problem, include the app/text field, transcription option,
selected language, installed version, and relevant errors. Do not include API keys.

## Remove Frame Voice

See [uninstall instructions](UNINSTALL.md). You can retain settings and models
independently, or remove both.
