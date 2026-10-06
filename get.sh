#!/bin/sh
# Download an authenticated private GitHub release and use the local installer.
set -eu
repo=techieyann/frame-voice
channel=stable
case "${1:-}" in
    --channel)
        channel=${2:?Choose stable or experimental}
        shift 2
        ;;
esac
case "$channel" in stable|experimental) ;; *) echo 'Choose stable or experimental.' >&2; exit 1;; esac
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = aarch64 ] || {
    echo 'Run this on the ARM64 SteamOS Frame.' >&2; exit 1;
}
command -v gh >/dev/null 2>&1 || { echo 'Install GitHub CLI and run gh auth login for this private repository.' >&2; exit 1; }
if [ "$channel" = stable ]; then
    tag=$(gh api "repos/$repo/releases/latest" --jq 'select(.draft == false and .prerelease == false) | .tag_name')
else
    tag=$(gh api --paginate --slurp "repos/$repo/releases?per_page=100" \
        --jq 'add | map(select(.draft == false and .prerelease == true and (.tag_name | test("^v[0-9]+\\.[0-9]+\\.[0-9]+-experimental\\.[0-9]+$")))) | sort_by(.published_at) | last | .tag_name // empty')
fi
[ -n "$tag" ] || { echo "No published $channel release exists yet." >&2; exit 1; }
case "$tag" in v*[!0-9a-zA-Z.-]*|''|*[!0-9a-zA-Z.-]*) echo 'Invalid release tag.' >&2; exit 1;; esac
asset="frame-voice-$tag-aarch64-unknown-linux-gnu.tar.gz"
download_dir=$(mktemp -d)
trap 'rm -rf "$download_dir"' EXIT HUP INT TERM
gh release download "$tag" --repo "$repo" --pattern "$asset" --pattern SHA256SUMS --dir "$download_dir"
cd "$download_dir"
# Verify only the expected file; never execute filenames/options from a checksum file.
expected=$(awk -v name="$asset" '$2 == name { print $1 }' SHA256SUMS)
[ "${#expected}" -eq 64 ] || { echo 'Missing or invalid release checksum.' >&2; exit 1; }
actual=$(sha256sum "$asset" | cut -d ' ' -f 1)
[ "$actual" = "$expected" ] || { echo 'Release checksum mismatch.' >&2; exit 1; }
python3 - "$asset" "$channel" "$tag" <<'PY'
import json
from pathlib import PurePosixPath
import sys
import tarfile
with tarfile.open(sys.argv[1]) as bundle:
    for member in bundle.getmembers():
        path = PurePosixPath(member.name)
        if path.is_absolute() or '..' in path.parts or not path.parts or path.parts[0] != 'frame-voice' or not (member.isfile() or member.isdir()):
            raise SystemExit('Invalid release archive member')
    metadata = json.load(bundle.extractfile('frame-voice/release.json'))
    if metadata.get('channel') != sys.argv[2] or metadata.get('version') != sys.argv[3] or metadata.get('target') != 'aarch64-unknown-linux-gnu':
        raise SystemExit('Release metadata does not match requested channel/version/architecture')
    bundle.extractall('.')
PY
cd frame-voice
sha256sum -c FILES-SHA256SUMS
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray "$@"
