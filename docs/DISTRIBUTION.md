# Stable and experimental channels

One source branch (`main`), two release channels:

| Channel | Tag example | GitHub release | Installer |
| --- | --- | --- | --- |
| Stable | `v0.1.0` | Normal release, marked Latest | `./get.sh --channel stable` |
| Experimental | `v0.1.0-experimental.1` | Prerelease, never Latest | `./get.sh --channel experimental` |

Stable resolves GitHub's latest non-prerelease. Experimental resolves the newest
published experimental prerelease by publication time. There are no mutable
channel tags and no automatic updates; users select a channel when installing.
The repository and its release assets remain private. Authenticated `gh` access
is required for downloads.

## Build action

`.github/workflows/release.yml` uses GitHub's native `ubuntu-22.04-arm` runner,
checks formatting/tests/Clippy, builds the daemon/helper and tray with glibc, then
packages only installable binaries, scripts, units, assets and OpenVR attribution.
OpenVR headers are pinned to SDK `v2.15.6`. The Frame supplies the SteamVR runtime;
it is not included. No handoffs, source history, tests or build directories are
shipped in the binary bundle.

Pushes to `main` build downloadable experimental artifacts without publishing.
Run **ARM64 distribution** manually in Actions to build a downloadable artifact
without publishing. Its version input defaults to `v0.1.0-experimental.1`.
Pushing a valid version tag builds and publishes the corresponding channel:

```sh
# Update both Cargo.toml versions and lockfiles to match the tag's base version.
git tag -a v0.1.0-experimental.1 -m 'Experimental 0.1.0, build 1'
git push origin v0.1.0-experimental.1
```

After testing the candidate on Frame, tag a qualified commit as stable:

```sh
git tag -a v0.1.0 -m 'Stable 0.1.0'
git push origin v0.1.0
```

No release is created simply by pushing `main`. CI verifies compilation and host
tests; microphone, controller, SteamVR overlay and text delivery behavior still
need on-device qualification before stable promotion. Runners must be enabled
and the account must have Actions minutes available for this private repository.

## Artifacts

- `frame-voice-vVERSION-aarch64-unknown-linux-gnu.tar.gz`
- `SHA256SUMS`

Inside the archive, `FILES-SHA256SUMS` covers installed files and `release.json`
records version, channel, architecture and source commit. The installer retains
release metadata and includes it in rollback snapshots. The project source is
available through Git/GitHub's source archives.

References: [GitHub ARM64 runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners),
[release/prerelease behavior](https://docs.github.com/en/rest/releases/releases).
