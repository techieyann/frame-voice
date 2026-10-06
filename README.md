<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/icons/frame-voice-header-dark.svg" />
    <img src="assets/icons/frame-voice-header-light.svg" width="300" height="56" alt="Frame Voice" />
  </picture>
</h1>

Voice dictation for Steam Frame. Speak into a text field using your controllers,
without a keyboard or an always-on microphone.

Works in the Desktop app and standalone desktop apps while Steam is in gamepad
mode. Dictation is unavailable in the Steam interface and VR games.

## Install on your Frame

1. Open **Desktop**, then open **Konsole** (the terminal).
2. Paste this command and press Enter:

   ```sh
   curl -fsSL https://raw.githubusercontent.com/techieyann/frame-voice/main/get.sh | sh
   ```

3. Follow setup to choose how your speech is transcribed. If asked, enter your
   SteamOS administrator password to allow text input. If you have never set
   that password, run `passwd` in Konsole first.
4. Find the Frame Voice microphone icon in the Desktop tray.

## Choose transcription and language

| Option | Speed | What you need | Why choose it |
| --- | --- | --- | --- |
| **Groq · Cloud** | Fastest | Internet and a [Groq API key](https://console.groq.com/keys) | Speech is sent to Groq for transcription; no local model download |
| **Local · Fast** | Fast | A model download of about 142 MiB | Transcription runs on your Frame and works offline after setup |
| **Local · Balanced** | Slow | A model download of about 466 MiB | Favors accuracy over speed and works offline after setup |

Choose **Language** in the Transcription tab, or **Automatic detection** if you
use more than one language. Local setup downloads the appropriate English or
multilingual model. Groq needs no language-specific download.

For accented characters or non-Latin scripts, use a standalone app such as
Firefox. These characters are not yet supported inside the nested Desktop.

## Try your first dictation

1. Open an app and focus the text field you want to fill.
2. Briefly touch either joystick cap, lift your thumb, then touch it again and hold.
3. Wait for the start tone and microphone badge, then speak.
4. Lift your thumb to finish. Wait for the transcription to appear.

Click the joystick during recording to cancel. After text appears, **A** submits
and releasing **B** clears; the left D-pad right/left do the same. Submit and clear are
available briefly after dictation and stop when you change focus. Nothing submits
automatically.

Open **Configure…** in the tray to customize Frame Voice. See the
[settings guide](docs/SETTINGS.md) for the General, Transcription, and Controllers tabs.

## Updates

Use **Configure… → General → Check for updates**. Updates preserve saved settings,
API keys, and downloaded models. See [updating Frame Voice](docs/SETTINGS.md#updates).

## If something does not work

- **No tone or microphone badge:** focus an app's text field, check that dictation
  is running in the tray, and try Restart dictation. A plain hold does not trigger
  recording; it must be tap, release, then hold.
- **Groq produces no text:** check your internet connection and API key.
- **Local setup fails:** check free storage and internet access for the first download.
- **Need more detail:** choose View recent logs in the tray, or see
  [troubleshooting and configuration](docs/CONFIGURATION.md).

## For developers and agents

The installer provides `./install-gui.sh` for GUI setup, `--gui` for Desktop
setup, and `--tui` for a terminal menu. Agents can use explicit flags without
interactive dialogs:

```sh
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray \
  --non-interactive --backend local-fast --language auto
```

For Groq, use `--backend groq --language en --groq-key-file /path/to/key`.
Noninteractive setup requires input permissions to be configured already.

`frame-voice --version` shows the version and build revision.
`frame-voice --check-updates --json` returns update status; use
`~/.local/share/frame-voice/update.sh --tui` for a guided terminal update.
See the [configuration reference](docs/CONFIGURATION.md) for advanced settings.

[Installation and removal](docs/INSTALLATION.md) ·
[Settings by tab](docs/SETTINGS.md) ·
[Builds and release channels](docs/DISTRIBUTION.md) · [MIT license](LICENSE)
