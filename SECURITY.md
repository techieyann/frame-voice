# Security policy

Frame Voice injects keystrokes into the focused application and, when Groq
transcription is selected, stores an API key in `~/.config/frame-voice/env`.
Reports about either of those paths are especially welcome.

## Reporting a vulnerability

Please do not open a public issue for security problems. Use GitHub's private
reporting at <https://github.com/techieyann/frame-voice/security/advisories/new>.

Include the Frame Voice version (`frame-voice --version`), the transcription
path in use, and steps to reproduce. You should hear back within a week.

## Supported versions

Only the latest stable release receives fixes. Experimental prereleases are
not supported once a newer build exists.
