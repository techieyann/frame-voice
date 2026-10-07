# Frame Voice design notes

This is an implementation reference. End users should start with
[settings](SETTINGS.md) or [troubleshooting](TROUBLESHOOTING.md).

## Capture and feedback

The accepted leading tap prearms microphone capture and a PulseAudio-compatible
speaker stream. The activation badge waits for capture/output readiness before
spending its growth interval. The start cue and animation use that clock, with a
playback lead. Speaker writes provide pacing; an additional per-write sleep would
cause underruns. The stream closes when the job and final cue finish.
`PAPLAY_BIN` retains the external-player override/fallback.

Headset feedback uses a growing blue microphone, a processing gear, and a red
crossed-out microphone for cancellation or insufficient speech. The Desktop tray
uses separate white running/disabled vectors, with the Frame corner mark.

## Speech evidence and ASR confidence

The local gate combines DC-free energy, an adaptive ambient floor, cue-frequency
rejection, and optional WebRTC VAD from SteamOS's
`libwebrtc-audio-processing-2`. Volume and VAD must agree on the same audio run
for `VOICE_MIN_SPEECH_MS`. A cue followed by unrelated noise cannot supply the
two kinds of evidence independently. No additional VAD model is downloaded.
Without the optional system library, the portable energy/cue gate remains.

Groq's [verbose response](https://console.groq.com/docs/speech-to-text) exposes
`avg_logprob` and `no_speech_prob`. A result is cancelled when every segment
reports a decoding score below `GROQ_ASR_MIN_LOGPROB` or no-speech probability
above `GROQ_ASR_MAX_NO_SPEECH`. These scores are not calibrated accuracy
percentages. Missing metadata retains the local audio gate. Debug logs include
numeric scores and decisions, never transcripts or keys.

No ordinary phrase, including “Yeah,” “you,” or “Thank you,” is banned by text.
Detection and transcription can still mistake environmental sounds for voice.

## Prompt handling

The managed custom prompt is `~/.config/frame-voice/groq-prompt.txt`; the shipped
neutral template is [assets/groq-prompt.txt](../assets/groq-prompt.txt).
`GROQ_PROMPT_FILE` can override its location. An absent file uses the embedded
neutral default. Cleanup loads the prompt for each request and preserves the
original language/script. ASR hints are separate (`VOICE_PROMPT_TEXT`).
User keys, custom prompts, and model downloads are excluded from releases.

## Context and delivery

Context observes the outer gamescope/X11 app focus. The built-in Desktop is a
nested Plasma session; an inner app/field change may not change that outer
context. ASCII text follows keyboard focus through uinput. Unicode uses a
clipboard helper for supported standalone apps with a known window class;
unsupported Desktop Unicode is refused rather than corrupted.

Desktop Clear backspaces the tracked insertion, including its separator.
Default right X adds no controller character. If right B is explicitly selected,
optional native-space accounting is scoped to its action origin and Desktop.
Controller Clear waits for release and a short settle interval. Changed context,
unavailable input, and expired pending actions cancel it.

The count belongs to the current process and resets at restart. Cursor movement
or unobserved inner-field edits can invalidate ownership. It is not a persistent
text-selection or accessibility model.

## Installer architecture

See [installer internals](INSTALLER.md) for transactional replacement, input
permissions, helper migration, and rollback. Packaging uses an explicit allowlist
and records checksums/source commit; release versions are documented in
[distribution](DISTRIBUTION.md).
