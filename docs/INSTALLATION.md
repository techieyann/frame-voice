# Installation

The release bundle includes Frame Voice, its tray and clipboard helper, ydotool/
ydotoold, an audio-only FFmpeg build, the Whisper engine, and microphone setup.
SteamOS supplies the SteamVR/PipeWire/ALSA runtime, Python, playback tools and
Desktop dialogs. Users do not install compilers or fetch these helpers separately.

## First install

```sh
curl -fsSL https://raw.githubusercontent.com/techieyann/frame-voice/main/get.sh | sh
```

Setup offers [Groq — Cloud, Local — Fast or Local — Balanced](CONFIGURATION.md#backend).
Groq requires a key from [Groq's console](https://console.groq.com/keys). Local
choices download and verify their models automatically. Input-device permissions
may require an administrator password once. Setup then starts dictation and the
Desktop tray. Run the installer as your normal user, not with sudo.

### GUI, terminal, and automated setup

The installer selects the GUI when Desktop is available and a terminal menu
otherwise. Explicit `--backend` flags keep the scripted setup path. For the new
release, choose a frontend explicitly:

```sh
./install-gui.sh
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray --gui
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray --tui
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray \
  --non-interactive --backend local-fast --language auto
```

The GUI and TUI offer transcription, language, general settings, and controller
mappings. For cloud automation, use `--backend groq --language en
--groq-key-file /path/to/key`. Noninteractive setup never opens selection/key
dialogs or administrator prompts; configure input permissions interactively first.
Canceling interactive setup restores the prior installation and service state.
Models downloaded before a failed/canceled setup may remain cached.

These frontend and language options are included in stable v0.1.0 and later.

Installation verifies archive and file checksums, channel and architecture before
replacement. Helpers live under `~/.local/share/frame-voice/bin`; existing shared
ydotool/Whisper installations and their services are untouched. Updates preserve
configuration and models, snapshot all managed files, and restore service state.
New snapshots never copy an existing config containing your API key.

When upgrading the old voice setup, add `--replace-legacy-helpers` to stop and
disable `ydotoold.service` and `voice-mixer.service` in favor of the bundled
helpers. Use this only if other applications no longer need those services.
Their files remain, and rollback or uninstall restores their previous service
state. For an extracted release:

```sh
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray --setup --enable-tray --replace-legacy-helpers
```

The keyboard-only input service clears `DISPLAY` because ydotoold's optional
[X11 setup](https://github.com/ReimuNotMoe/ydotool/blob/v1.0.4/Daemon/ydotoold.c#L333-L349)
only adjusts mouse acceleration. This avoids xinput/Xwayland warnings without
changing keyboard injection.

## Channels and noninteractive setup

```sh
curl -fsSL https://raw.githubusercontent.com/techieyann/frame-voice/main/get.sh | sh -s -- --channel experimental
```

For unattended use, add `--backend local-fast`, `--backend local-balanced`, or
`--backend groq --groq-key-file /path/to/key`. Models must download on the first
local installation; cached verified models need no connection. A terminal or
Desktop dialog is required if no backend/key is supplied.

## Tray

Open Desktop for status, Pause/Resume, Restart, configuration and logs. The tray
runs on Plasma's private bus; service controls use the outer user bus. Hiding the
tray leaves dictation running. Configuration changes restart dictation.

## Rollback and removal

From a source checkout or extracted release directory:

```sh
./rollback.sh
./uninstall.sh
# To also remove Frame Voice settings and API keys:
./uninstall.sh --remove-config
# Also remove downloaded models (a full removal):
./uninstall.sh --remove-config --remove-models
```

Rollback restores the complete last snapshot and prior service state. Modified
managed files are not overwritten. Uninstall removes application files, units,
app data, and installation snapshots. Configuration/keys and model downloads are
retained by default, controlled independently by `--remove-config` and
`--remove-models`. Models downloaded by this tool are under
`~/.local/share/frame-voice/models`; older shared Whisper models are not deleted.

The tray's **Configure… → General → Uninstall…** action provides confirmation and two retention
checkboxes, both on by default. Unchecking both also removes
`~/.config/frame-voice`, downloaded models, and this installer's input permission
and module-loading rules if present. Removing administrator files requires
administrator authentication; cancellation leaves the app installed. Modified
administrator rules require manual removal. Older voice tools and their files
remain untouched.

## Building

Use the native ARM64 [distribution workflow](DISTRIBUTION.md) for a complete
bundle. Development builds need current stable Rust, C/C++, CMake, pkg-config,
libpulse headers, scdoc and matching OpenVR headers. The workflow checks out
pinned helper sources under `.build/`; run `./build-local.sh` and
`./tools/build-runtime.sh`, then `./install.sh --setup`. Production builds use
glibc. The installer can read built helpers from `.build/runtime/bin`.

## Diagnostics

```sh
~/.local/bin/frame-voice --check
systemctl --user status frame-voice frame-voice-input frame-voice-mixer frame-voice-tray-session
journalctl --user -u frame-voice -n 50 --no-pager
```
