# Advanced configuration reference

For the GUI, read [Settings by tab](SETTINGS.md). This page is for developers and
agents editing `~/.config/frame-voice/env`. Use literal `KEY=VALUE` entries, not
shell commands; keep configuration/key files mode 600. Restart dictation after
editing the file.

Values load from the compatible `~/.config/voice-prompt/env`, then
`~/.config/frame-voice/env`, then process environment; later values win.

## Controls

Tap a joystick cap, release, then touch it again and hold to record. Release ends
recording. A plain hold does nothing. Click the stick during recording to cancel.
In the headset, the blue microphone grows during startup; the spinning gear indicates processing;
the red crossed-out mic indicates cancellation or insufficient speech.

After successful dictation, A submits and X clears after the button is released; the left D-pad right/left
provide submit/clear. These actions expire after ten seconds or a focus change.
Unknown targets refuse clear rather than guessing. Speech commands match exact
whole utterances: "submit", "send", "enter"; "scratch that", "clear", "undo";
and "cancel" or "never mind". Ordinary phrases remain text.


The Desktop tray is separate: white microphone when running, white crossed-out
microphone when disabled.
## Common settings

| Setting | Default | Purpose |
| --- | --- | --- |
| `VOICE_BACKEND` | `groq` | `groq` or `local`; setup selects the local model |
| `VOICE_LANG` | `en` | Language code or `auto` for automatic detection |
| `VOICE_MODEL` | Set by local setup | Verified local model file selected by the installer |
| `VOICE_INJECT` | `type` | `type` or clipboard `paste` |
| `VOICE_PASTE_CHUNK` | `0` | Maximum characters per paste; 0 disables splitting |
| `VOICE_TRAILING` | `space` | Separator after dictation (`space` or `none`) |
| `VOICE_BEEP` | `1` | Feedback tones |
| `VOICE_NOTIFY` | `1` | Headset microphone badges |
| `VOICE_CLEANUP` | `1` | Optional Groq text cleanup |
| `VOICE_SLASH` | `0` | Optional interpretation of spoken "slash" as `/` during cleanup |
| `GROQ_USAGE_NOTIFY` | `1` | Groq usage/limit notifications; separate from microphone badges |
| `VOICE_MAX` | `30` | Maximum recording duration, seconds |
| `VOICE_THRESHOLD` | `300` | Minimum audio level |
| `GROQ_ASR_MIN_LOGPROB` | `-0.75` | Minimum Groq segment decoding score (closer to zero is stronger) |
| `GROQ_ASR_MAX_NO_SPEECH` | `0.6` | Maximum Groq segment no-speech probability |
| `VOICE_MIN_SPEECH_MS` | `200` | Contiguous speech required before transcription |
| `VRBTN_CLEAR_B_SPACE` | `1` | Include the native space typed by right-hand B in Desktop Clear; disable for a customized B mapping |
| `VRBTN_ARM_MS` | `150` | Hold threshold, milliseconds |
| `VRBTN_TAP_MS` | `250` | Maximum first-tap duration, milliseconds |
| `VRBTN_READY_MS` | `600` | Interval allowed before the second touch |
| `VRBTN_GROW_MS` | `180` | Microphone growth after capture and speaker startup, milliseconds; at least hold threshold plus playback lead |
| `VRBTN_BEEP_LEAD_MS` | `30` | Playback lead time, milliseconds |
| `VOICE_ACTION_WINDOW_MS` | `10000` | Submit/clear activation window |

Per-hand mappings: `VRBTN_DICTATE_LEFT/RIGHT`, `VRBTN_SUBMIT_LEFT/RIGHT`,
`VRBTN_CLEAR_LEFT/RIGHT`. The tray offers valid button choices; changes regenerate
the controller binding on service restart. More settings and validation are in
`src/config.rs` and the installer-generated config template.


Backend flags are documented in the [developer guide](DEVELOPMENT.md).
For signal gating, prompt handling, and text delivery internals, see
[design notes](DESIGN.md). Diagnostics are maintained in
[troubleshooting](TROUBLESHOOTING.md#diagnostics).
