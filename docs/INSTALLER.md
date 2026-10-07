# Installer internals

The release bundle includes Frame Voice, its tray and clipboard helper, ydotool/
ydotoold, an audio-only FFmpeg build, the Whisper engine, and microphone setup.
SteamOS supplies the SteamVR/PipeWire/ALSA runtime, Python, playback tools and
Desktop dialogs. Users do not install compilers or fetch these helpers separately.

## Entry points

User installation is documented in the [README](../README.md); GUI/terminal/agent
entry points are in [Development](DEVELOPMENT.md). The default downloader opens
the GUI when Desktop is available and the terminal menu otherwise. Explicit
backend flags retain the scripted setup path.

Canceling interactive setup restores the previous installation/service state.
Models downloaded before a failed/canceled setup may remain cached.

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

## Tray

Open Desktop for status, Pause/Resume, Restart, configuration and logs. The tray
runs on Plasma's private bus; service controls use the outer user bus. Hiding the
tray leaves dictation running. Configuration changes restart dictation.

## Rollback and removal

User-facing retention choices are in [Uninstall](UNINSTALL.md); terminal flags
are in [Development](DEVELOPMENT.md#terminal-rollback-and-removal).
Rollback restores the last snapshot and previous service state while preserving
modified files. Removal deletes managed application data, units, and snapshots.
Configuration and managed model downloads are independently retained by default.
Older/shared tools and their model directories are not removed.

## Building

Use the native ARM64 [distribution workflow](DISTRIBUTION.md) for a complete
bundle. Development builds need current stable Rust, C/C++, CMake, pkg-config,
libpulse headers, scdoc and matching OpenVR headers. The workflow checks out
pinned helper sources under `.build/`; run `./build-local.sh` and
`./tools/build-runtime.sh`, then `./install.sh --setup`. Production builds use
glibc. The installer can read built helpers from `.build/runtime/bin`.

## Diagnostics

Use the [canonical diagnostic commands](TROUBLESHOOTING.md#diagnostics).
