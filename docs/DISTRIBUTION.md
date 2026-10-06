# Stable and experimental channels

One source branch (`main`), two release channels:

| Channel | Tag example | GitHub release | Installer |
| --- | --- | --- | --- |
| Stable | `v0.1.0` | Normal release, marked Latest | `./get.sh --channel stable` |
| Experimental | `v0.1.0-experimental.1` | Prerelease, never Latest | `./get.sh --channel experimental` |

Stable resolves GitHub's latest non-prerelease. Experimental resolves the newest
published experimental prerelease by publication time. There are no mutable
channel tags and no automatic updates; users select a channel when installing.
The public installer downloads release assets over HTTPS without requiring GitHub
CLI or a login. The repository must be public before publishing that install URL.
During private development, `GH_TOKEN` can authenticate the downloader.

## Build action

`.github/workflows/release.yml` uses GitHub's native `ubuntu-22.04-arm` runner,
checks formatting/tests/Clippy, builds the daemon/helper and tray with glibc, then
packages the application, input helpers, capture tool, local engine, setup scripts,
units, assets, third-party licenses and corresponding helper sources.
OpenVR headers are pinned to SDK `v2.15.6`. Runtime builds use ydotool `v1.0.4`,
whisper.cpp `v1.9.4` and a minimal FFmpeg `n8.0.3` build with Pulse input. The Frame supplies the SteamVR runtime;
it is not included. Development notes, project tests and build directories are excluded from the
binary bundle. Helper sources accompany their separately licensed binaries.

Ordinary branch pushes do not run CI or consume build minutes. Release tags run
the checks and build before publishing.
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

Pushing `main` does not build or publish anything. Release CI verifies compilation and host
tests; microphone, controller, SteamVR overlay and text delivery behavior still
need on-device qualification before stable promotion. Runners must be enabled
and the account must have Actions minutes available for this private repository.

## Artifacts

Frame Voice's code is [MIT licensed](../LICENSE). Separately bundled ydotool,
Whisper and FFmpeg retain their upstream licenses; the archive includes their
license text and corresponding sources.

- `frame-voice-vVERSION-aarch64-unknown-linux-gnu.tar.gz`
- `SHA256SUMS`

Inside the archive, `FILES-SHA256SUMS` covers installed files and `release.json`
records version, channel, architecture and source commit. The installer retains
release metadata and includes it in rollback snapshots. The project source is
available through Git/GitHub's source archives.

References: [GitHub ARM64 runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners),
[release/prerelease behavior](https://docs.github.com/en/rest/releases/releases).
