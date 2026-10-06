# Configuration and controls

Configure Frame Voice and controller mappings from the Desktop tray, or edit
`~/.config/frame-voice/env`. Settings are literal `KEY=VALUE`, never shell commands.
Restart `frame-voice` after changing them. Configuration and key files should be
mode 600.

## Backend

First-run setup offers three choices:

| Choice | Use | What setup does |
| --- | --- | --- |
| Groq — Cloud | Fast online transcription | Requests your [Groq API key](https://console.groq.com/keys) |
| Local — Fast | Offline English transcription, favoring speed | Downloads and verifies `base.en` (~142 MiB) |
| Local — Balanced | Offline English transcription, more detail with more processing | Downloads and verifies `small.en` (~466 MiB) |

Local choices need no Groq key and work offline after the download. The Whisper
engine is included. Existing verified models are reused. The key is saved only
in your mode-600 config and is never printed. Updates preserve your current choice.
To switch, rerun the installer with `--backend groq`, `--backend local-fast` or
`--backend local-balanced`. `--groq-key-file PATH` reads a key without placing it
in command arguments. Local English presets select `VOICE_LANG=en`.

Configuration loads from the compatible `~/.config/voice-prompt/env`, then
`~/.config/frame-voice/env`, then process environment; later values win.

## Controls

Tap a joystick cap, release, then touch it again and hold to record. Release ends
recording. A plain hold does nothing. Click the stick during recording to cancel.
The blue microphone grows during startup; the spinning gear indicates processing;
the red crossed-out mic indicates cancellation or insufficient speech.

After successful dictation, A submits and B clears; the left D-pad right/left
provide submit/clear. These actions expire after ten seconds or a focus change.
Unknown targets refuse clear rather than guessing. Speech commands match exact
whole utterances: "submit", "send", "enter"; "scratch that", "clear", "undo";
and "cancel" or "never mind". Ordinary phrases remain text.

## Common settings

| Setting | Default | Purpose |
| --- | --- | --- |
| `VOICE_INJECT` | `type` | `type` or clipboard `paste` |
| `VOICE_PASTE_CHUNK` | `0` | Maximum characters per paste; 0 disables splitting |
| `VOICE_TRAILING` | `space` | Separator after dictation (`space` or `none`) |
| `VOICE_BEEP` | `1` | Feedback tones |
| `VOICE_NOTIFY` | `1` | Headset badge and usage warnings |
| `VOICE_CLEANUP` | `1` | Optional Groq text cleanup |
| `VOICE_SLASH` | `1` | Interpret spoken "slash" as `/` during cleanup |
| `VOICE_MAX` | `30` | Maximum recording duration, seconds |
| `VOICE_THRESHOLD` | `300` | Minimum audio level |
| `VOICE_MIN_SPEECH_MS` | `200` | Contiguous speech required before transcription |
| `VRBTN_ARM_MS` | `150` | Hold threshold, milliseconds |
| `VRBTN_TAP_MS` | `250` | Maximum first-tap duration, milliseconds |
| `VRBTN_READY_MS` | `600` | Interval allowed before the second touch |
| `VRBTN_GROW_MS` | `450` | Microphone growth duration, milliseconds |
| `VRBTN_BEEP_LEAD_MS` | `150` | Playback lead time, milliseconds |
| `VOICE_ACTION_WINDOW_MS` | `10000` | Submit/clear activation window |

Per-hand mappings: `VRBTN_DICTATE_LEFT/RIGHT`, `VRBTN_SUBMIT_LEFT/RIGHT`,
`VRBTN_CLEAR_LEFT/RIGHT`. The tray offers valid button choices; changes regenerate
the controller binding on service restart. More settings and validation are in
`src/config.rs` and the installer-generated config template.

## Troubleshooting

```sh
~/.local/bin/frame-voice --check
systemctl --user status frame-voice frame-voice-tray-session
journalctl --user -u frame-voice -n 50 --no-pager
```

`--check` validates configuration and compiled OpenVR support, not credentials,
network access or headset health. Focus changes cancel pending results; text
already delivered cannot be recalled. Silence makes no transcription request.
