# Developer and agent guide

End-user setup is in the [README](../README.md). GUI settings and common failures
are covered by [Settings](SETTINGS.md) and [Troubleshooting](TROUBLESHOOTING.md).

## Guided and automated installation

The default downloader selects the GUI when Desktop is open, including a
Desktop session discovered from SSH. Without Desktop it opens the terminal menu.
Use `--tui` to explicitly select a terminal menu. Explicit backend flags use the
scripted path; `--non-interactive` forbids prompts.

From an extracted bundle or native build:

```sh
./install-gui.sh
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray --gui
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray --tui
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray \
  --non-interactive --backend local-fast --language auto
```

For **Groq · Cloud**, use `--backend groq --language en --groq-key-file /path/to/key`.
The local flags are `local-fast` (**Local · Fast**) and `local-balanced`
(**Local · Balanced**). Configure input permissions interactively before unattended
setup. Noninteractive mode never opens administrator/key-selection dialogs.
Models need their initial download; verified cached files are reused.

## Version and updates

```sh
frame-voice --version
frame-voice --check-updates --json
~/.local/share/frame-voice/update.sh --tui
```

Build info includes package version, source revision, dirty marker, and compiled
capability. Installed release metadata is trusted only when its commit matches
the binary. Unpackaged builds are displayed as development versions.

## Terminal rollback and removal

From a checkout or extracted release:

```sh
./rollback.sh
./uninstall.sh
./uninstall.sh --remove-config --remove-models
```

Removal keeps configuration/models by default; `--remove-config` and
`--remove-models` control them separately. GUI removal is documented in
[Uninstall](UNINSTALL.md). Rollback restores the last managed snapshot and service
state, preserving files modified after installation.

## Build and architecture references

- [Installer internals and legacy migration](INSTALLER.md)
- [Advanced configuration variables](CONFIGURATION.md)
- [Capture, speech gating, prompts, and delivery](DESIGN.md)
- [ARM64 builds and release channels](DISTRIBUTION.md)
- [Canonical diagnostic commands](TROUBLESHOOTING.md#diagnostics)

Use current stable Rust and native ARM64 Linux with C/C++, CMake, pkg-config,
libpulse headers, scdoc, and the pinned OpenVR headers. `./build-local.sh` builds
the application/tray. `./tools/build-runtime.sh` builds bundled helpers from pinned
sources under `.build/`. The distribution workflow creates a glibc bundle.
