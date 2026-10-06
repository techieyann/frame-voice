# Installation and removal

Run on an aarch64 SteamOS Frame as its normal user, without sudo. Runtime needs
SteamVR, gamescope/Xwayland, Python 3, ffmpeg with Pulse input, `pw-play` or
`paplay`, `ydotool`/`ydotoold`, and the microphone setup provided by `voice-mixer`.
The installer does not create device permissions or external services. The
optional config form uses GTK4/PyGObject, with zenity as a fallback.

## Prebuilt channels

The private repository requires an authenticated GitHub CLI with repository
access. Once releases exist:

```sh
curl -fsSL -H "Authorization: Bearer $(gh auth token)" \
  -H 'Accept: application/vnd.github.raw+json' \
  https://api.github.com/repos/techieyann/frame-voice/contents/get.sh | sh -s -- --enable-tray
```

To opt into experimental builds, append `--channel experimental --enable-tray`
after `sh -s --` instead. Authentication is required while the repository and
assets remain private; an SSH key alone does not authenticate HTTP downloads.
The installer downloads binaries and runs the local installer without cloning
source or compiling. A release must be published before its channel is installable.

Downloads are verified against SHA-256 and channel/architecture metadata before
installation. No compiler is needed. Re-run the command to update or switch
channels; configuration is retained. Stable never falls back to experimental.
Channel releases are described in [Distribution](DISTRIBUTION.md).

## From source

Rust (current stable), a C++ compiler, and matching OpenVR headers are required. Builds use
Linux glibc, not musl:

```sh
git clone git@github.com:techieyann/frame-voice.git
cd frame-voice
./build-local.sh
./install.sh --enable-tray
```

`OPENVR_INCLUDE_DIR` can override the SteamVR header directory. Daemon and tray
lockfiles are separate. `./install.sh --daemon-only` installs only the daemon and
clipboard helper. Custom binaries can be passed as the first argument and with
`--tray-binary PATH`.

## Configure and activate

First installation creates a commented mode-600 config template without requiring
credentials. [Configure a backend](CONFIGURATION.md#backend), then:

```sh
~/.local/bin/frame-voice --check
systemctl --user enable --now frame-voice frame-voice-tray-session
```

`--enable-tray` starts only the session watcher. `--activate` also validates the
backend configuration and starts dictation. Existing enabled/running service state
is preserved during updates; running services restart, interrupting any recording.
All managed binaries, assets and units are snapshotted before replacement.

## Desktop tray

The independent companion launches inside the nested Plasma session, using its
actual private bus. Service controls use the outer user bus. Open Desktop to see
status, Start/Resume, Pause, Restart, configuration forms and logs. Pause stops
dictation; hiding the tray does not. Hiding lasts until Desktop is reopened.

## Rollback and uninstall

From the source checkout:

```sh
./rollback.sh
./uninstall.sh
```

Rollback restores the last complete snapshot and prior service state. It refuses
to overwrite newer modified managed files. Uninstall removes unchanged managed
files and stops their units; configuration, keys, backups, `ydotoold`, `voice-mixer`
and unrelated software remain. Snapshots live under
`~/.local/share/frame-voice-installer/transactions/`.
