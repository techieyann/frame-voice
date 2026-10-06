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

Run this on your Frame:

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

| Option | What you need | Why choose it |
| --- | --- | --- |
| **Groq · Cloud** | Internet and a [Groq API key](https://console.groq.com/keys) | Speech is sent to Groq for transcription; no local model download |
| **Local · Fast** | A model download of about 142 MiB | Transcription runs on your Frame and works offline after setup |
| **Local · Balanced** | A model download of about 466 MiB | Favors accuracy over speed and works offline after setup |

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

## Change settings or free space

Open the tray icon and choose **Configure…** to open **Frame Voice Configuration**:

- **General:** tones, text delivery, and recording options.
- **Transcription:** Groq or local transcription, language, and downloaded models.
- **Controllers:** enable each hand and choose submit/clear buttons.

Make changes across tabs, then use **Save settings** in the shared footer.
Save becomes available when you change a setting.

To switch transcription paths, choose the new path and **Save settings**. Your
Groq API key stays saved when switching to local transcription.

To free model storage without uninstalling, use **Delete…** under Downloaded
models. The active model is protected: save another transcription path first.
Deletion asks for confirmation; selecting that model later downloads it again.

To uninstall, open **General** and click **Uninstall…** at the bottom.
The confirmation offers separate **Keep current configuration** and
**Keep downloaded models** choices. Uncheck both to remove the
app's settings, saved keys, downloaded models, and app data. Installation backups
are removed during uninstall. Older, separate voice tools and their models are
left alone.

Under Groq, **Edit cleanup prompt…** opens an editor in the settings window.
Use **Apply**, then **Save settings** to save your instructions. **Reset to default**
restores the shipped prompt in the editor. Your custom prompt stays on your Frame.
Groq notification and optional spoken-slash settings are there too.

The tray shows a blue microphone while dictation runs and a red crossed-out
microphone when paused or stopping. Feedback audio is warmed during the leading
tap and reused for start, stop, and cancel cues; it closes after the job finishes.
The small microphone badge waits for audio startup, then grows toward the start
tone. Wait for the tone before speaking.

Quiet recordings are checked for voice before transcription. Room noise and
cue tones should cancel without inserting text. Groq results with weak decoding
or high no-speech scores also cancel; genuine short phrases remain valid.

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

[Installation and removal](docs/INSTALLATION.md) ·
[Configuration](docs/CONFIGURATION.md) ·
[Builds and release channels](docs/DISTRIBUTION.md) · [MIT license](LICENSE)
