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

Installation verifies archive and file checksums, channel and architecture before
replacement. Helpers live under `~/.local/share/frame-voice/bin`; existing shared
ydotool/Whisper installations and their services are untouched. Updates preserve
configuration and models, snapshot all managed files, and restore service state.
New snapshots never copy an existing config containing your API key.

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
```

Rollback restores the complete last snapshot and prior service state. Modified
managed files are not overwritten. Uninstall removes unchanged application files
and units; configuration, keys, model downloads and snapshots remain. One-time uinput
permission/module-loading rules are retained so other tools using the device keep working.

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
