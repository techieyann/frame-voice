# Frame Voice

Controller-triggered dictation for Steam Frame.

## Requirements

- Steam Frame (aarch64 SteamOS), SteamVR, and gamescope/Xwayland.
- Python 3, ffmpeg, `pw-play` or `paplay`, `ydotool`/`ydotoold`, and `voice-mixer`.
- Authenticated GitHub CLI for private downloads; a Groq API key or local Whisper model.

## Install

Install a published stable build on the Frame without sudo:

```sh
curl -fsSL -H "Authorization: Bearer $(gh auth token)" \
  -H 'Accept: application/vnd.github.raw+json' \
  https://api.github.com/repos/techieyann/frame-voice/contents/get.sh | sh -s -- --enable-tray
```

Configure the backend through the Desktop tray. [Activation and source builds](docs/INSTALLATION.md).

[Configuration](docs/CONFIGURATION.md) · [Installation and removal](docs/INSTALLATION.md) · [Stable and experimental builds](docs/DISTRIBUTION.md)
