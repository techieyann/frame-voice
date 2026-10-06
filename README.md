<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/icons/frame-voice-header-dark.svg" />
    <img src="assets/icons/frame-voice-header-light.svg" width="300" height="56" alt="Frame Voice" />
  </picture>
</h1>

Voice-to-text input for Steam Frame. Tap either joystick cap, then touch and hold.
Speak after the tone; release to transcribe or click the stick to cancel.
Press **A** to submit or **B** to clear. Nothing submits automatically.

Supports the Desktop app and standalone desktop apps while Steam is in gamepad
mode. Dictation cannot be triggered in the Steam UI or VR games.

## Install

Run on the Frame:

```sh
curl -fsSL https://raw.githubusercontent.com/techieyann/frame-voice/main/get.sh | sh
```

During setup, choose your transcription helper:

- **Groq** — [API key](https://console.groq.com/keys) and internet required.
- **Local Fast** — ~2 seconds.
- **Local Balanced** — ~8 seconds, but more accurate.

[Installation details](docs/INSTALLATION.md).

[Configuration](docs/CONFIGURATION.md) · [Installation and removal](docs/INSTALLATION.md) · [Stable and experimental builds](docs/DISTRIBUTION.md) · [MIT license](LICENSE)
