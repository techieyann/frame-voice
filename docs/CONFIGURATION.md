# Configuration reference

For the configuration window, read [Settings by tab](SETTINGS.md). This reference
covers terminal configuration, advanced options, and diagnostics.

You can also edit
`~/.config/frame-voice/env`. Settings are literal `KEY=VALUE`, never shell commands.
Restart `frame-voice` after changing them. Configuration and key files should be
mode 600.

## Backend

First-run setup offers three choices:

| Choice | Use | What setup does |
| --- | --- | --- |
| Groq — Cloud | Fast online transcription | Requests your [Groq API key](https://console.groq.com/keys) |
| Local — Fast | Offline transcription, favoring speed | Downloads and verifies `base.en` or multilingual `base` (~142 MiB) |
| Local — Balanced | Offline transcription, more detail with more processing | Downloads and verifies `small.en` or multilingual `small` (~466 MiB) |

Local choices need no Groq key and work offline after the download. The Whisper
engine is included. Existing verified models are reused. The key is saved only
in your mode-600 config and is never printed. Updates preserve your current choice.
Switch in the Transcription tab, or rerun the installer with `--backend groq`, `--backend local-fast` or
`--backend local-balanced`. `--groq-key-file PATH` reads a key without placing it
in command arguments. Local English presets select `VOICE_LANG=en`.

The searchable Language menu offers automatic detection and 99 named languages.
English uses the `.en` local model; other languages and automatic detection use
multilingual models. Set `--language CODE` during installation or `VOICE_LANG`
in configuration (`en`, `es`, `fr`, `ja`, etc.; `auto` for detection). Groq omits
its optional language parameter for automatic detection; whisper-cli receives
`-l auto`. Cleanup is instructed to preserve the original language and script.
[Groq language parameters](https://console.groq.com/docs/speech-to-text) ·
[Whisper model variants and checksums](https://github.com/ggml-org/whisper.cpp/blob/v1.9.4/models/README.md).

Non-ASCII transcripts use clipboard paste automatically in standalone apps with
a known window class. Nested Desktop's classless target still cannot safely
receive Unicode through keyboard injection; that path refuses Unicode instead
of typing corrupted text. Spoken action aliases remain English; A/B and D-pad
controls work independently of transcription language. Models for both variants
appear separately in Downloaded models and can be deleted when unused.

Configuration loads from the compatible `~/.config/voice-prompt/env`, then
`~/.config/frame-voice/env`, then process environment; later values win.

## Controls

Tap a joystick cap, release, then touch it again and hold to record. Release ends
recording. A plain hold does nothing. Click the stick during recording to cancel.
The blue microphone grows during startup; the spinning gear indicates processing;
the red crossed-out mic indicates cancellation or insufficient speech.

After successful dictation, A submits and B clears after the button is released; the left D-pad right/left
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

## Groq prompt customization

**Edit cleanup prompt…** under Groq opens the built-in editor. Apply your draft,
then use the shared Save settings button to write
`~/.config/frame-voice/groq-prompt.txt`. Reset to default restores the shipped
text in the editor; canceling discards the editor draft. The shipped template is
[`assets/groq-prompt.txt`](../assets/groq-prompt.txt): neutral dictation cleanup,
original-language preservation, and no invented content. It is loaded for each
cleanup request, so edits apply to the next dictation. `GROQ_PROMPT_FILE` can
override its location. An absent file uses the neutral embedded default.

Feedback keeps one PulseAudio-compatible speaker stream warm for the current
dictation job, instead of opening a player per cue. It releases the stream when
the job and final cue finish; the microphone is still gesture-controlled.
The small activation badge waits for capture and speaker startup before growing.
Its growth and start cue then share a clock, accounting for a cold first stream
without adding a fixed delay to already-warm recordings. With tones disabled,
only capture readiness is required.
`PAPLAY_BIN` retains the external-player override/fallback behavior.

Silence detection combines an adaptive volume floor, cue-tone rejection, and
WebRTC voice activity detection from SteamOS's existing
`libwebrtc-audio-processing-2` library. Voice and volume evidence must agree on
the same audio for `VOICE_MIN_SPEECH_MS`; a cue followed by unrelated noise cannot
satisfy the two checks independently. No extra speech model is downloaded.
If the optional system library is unavailable, the portable volume/cue gate remains
active. `VRBTN_DEBUG=1` logs the gate decision and VAD result without transcripts.
No ordinary phrase such as “Yeah,” “you,” or “Thank you” is blocked by text.
Groq's [detailed response](https://console.groq.com/docs/speech-to-text) provides
`avg_logprob` and `no_speech_prob`. After the local gate, Frame Voice cancels if
every returned segment has a decoding score below `GROQ_ASR_MIN_LOGPROB` or a
no-speech probability above `GROQ_ASR_MAX_NO_SPEECH`. These are diagnostic scores,
not calibrated accuracy percentages. Missing scores retain the local audio gate.
Debug logging includes numeric scores and decisions, never transcript text.

Desktop clear counts the trailing separator too. For right-hand B with the default
Steam layout, it also counts the space B itself types into Desktop. Other bindings
and spoken Clear do not add that character. Set `VRBTN_CLEAR_B_SPACE=0` if your
custom Steam B mapping does not type a space. Its count belongs to the
running daemon, so a service restart resets that history. Test clear using a
fresh dictation after changing settings or restarting the service.

Spoken-slash conversion is an optional toggle and is off by default; personal
preferences belong in user configuration. ASR hints are separate and remain empty
by default (`VOICE_PROMPT_TEXT`). API keys, personalized prompt files, and model
downloads are excluded from release packaging.

## Troubleshooting

```sh
~/.local/bin/frame-voice --check
systemctl --user status frame-voice frame-voice-tray-session
journalctl --user -u frame-voice -n 50 --no-pager
```

`--check` validates configuration and compiled OpenVR support, not credentials,
network access or headset health. Focus changes cancel pending results; text
already delivered cannot be recalled. Silence makes no transcription request.

## Version and updates

`frame-voice --version` and `--build-info` report the package version, source
revision, dirty-source marker, and compiled OpenVR capability. Release metadata
is trusted only when its source commit matches the binary. Unpackaged/local
builds are shown as development versions.

Use General → Check for updates, or `frame-voice --check-updates --json`.
Checks compare stable release versions; they neither select experimental releases
nor downgrade newer versions. The GUI's Update action opens the transactional
installer after settings are saved. `~/.local/share/frame-voice/update.sh --tui`
provides the terminal path. No background polling or automatic update is enabled.
