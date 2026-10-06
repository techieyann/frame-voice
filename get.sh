#!/bin/sh
# Public binary installer; GH_TOKEN is optional for private development testing.
set -eu
channel=stable
if [ "${1:-}" = --channel ]; then
    channel=${2:?Choose stable or experimental}
    shift 2
fi
case "$channel" in stable|experimental) ;; *) echo 'Choose stable or experimental.' >&2; exit 1;; esac
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = aarch64 ] || {
    echo 'Run this on the ARM64 SteamOS Frame.' >&2; exit 1;
}
command -v python3 >/dev/null 2>&1 || { echo 'SteamOS Python 3 is required.' >&2; exit 1; }
download_dir=$(mktemp -d)
trap 'rm -rf "$download_dir"' EXIT HUP INT TERM
python3 - "$channel" "$download_dir" <<'PY'
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import sys
import tarfile
import urllib.request

repo = 'techieyann/frame-voice'
channel, directory = sys.argv[1], Path(sys.argv[2])
api = f'https://api.github.com/repos/{repo}/releases'
token = os.environ.get('GH_TOKEN') or os.environ.get('GITHUB_TOKEN')

def request(url, binary=False):
    headers = {'Accept': 'application/octet-stream' if binary else 'application/vnd.github+json',
               'User-Agent': 'frame-voice-installer'}
    message = urllib.request.Request(url, headers=headers)
    if token and url.startswith('https://api.github.com/'):
        # Never forward credentials to the storage host on an asset redirect.
        message.add_unredirected_header('Authorization', f'Bearer {token}')
    return urllib.request.urlopen(message, timeout=60)

def release_json(url):
    with request(url) as response: return json.load(response)

if channel == 'stable':
    release = release_json(api + '/latest')
    if release.get('draft') or release.get('prerelease'): raise SystemExit('No stable release available')
else:
    candidates, page = [], 1
    while True:
        batch = release_json(f'{api}?per_page=100&page={page}')
        candidates.extend(item for item in batch if not item.get('draft') and item.get('prerelease')
                          and re.fullmatch(r'v\d+\.\d+\.\d+-experimental\.[1-9]\d*', item.get('tag_name', '')))
        if len(batch) < 100: break
        page += 1
    if not candidates: raise SystemExit('No published experimental release exists yet')
    release = max(candidates, key=lambda item: item.get('published_at', ''))
tag = release['tag_name']
if not re.fullmatch(r'v\d+\.\d+\.\d+(?:-experimental\.[1-9]\d*)?', tag): raise SystemExit('Invalid release version')
asset = f'frame-voice-{tag}-aarch64-unknown-linux-gnu.tar.gz'
for name in (asset, 'SHA256SUMS'):
    match = next((item for item in release['assets'] if item['name'] == name), None)
    if match is None: raise SystemExit(f'Release is missing {name}')
    url = match['url'] if token else match['browser_download_url']
    allowed = f'https://api.github.com/repos/{repo}/releases/assets/' if token else f'https://github.com/{repo}/releases/download/'
    if not url.startswith(allowed): raise SystemExit('Invalid release asset URL')
    with request(url, binary=bool(token)) as response, (directory/name).open('wb') as output:
        shutil.copyfileobj(response, output)
checksums = [line.split() for line in (directory/'SHA256SUMS').read_text().splitlines()]
expected = [fields[0] for fields in checksums if len(fields) == 2 and fields[1] == asset]
if len(expected) != 1 or not re.fullmatch(r'[0-9a-f]{64}', expected[0]): raise SystemExit('Invalid release checksum')
digest = hashlib.sha256()
with (directory/asset).open('rb') as stream:
    for chunk in iter(lambda: stream.read(1024*1024), b''): digest.update(chunk)
if digest.hexdigest() != expected[0]: raise SystemExit('Release checksum mismatch')
with tarfile.open(directory/asset) as bundle:
    for member in bundle.getmembers():
        path = PurePosixPath(member.name)
        if path.is_absolute() or '..' in path.parts or not path.parts or path.parts[0] != 'frame-voice' or not (member.isfile() or member.isdir()):
            raise SystemExit('Invalid release archive member')
    metadata = json.load(bundle.extractfile('frame-voice/release.json'))
    if metadata.get('installer_version', 0) < 2:
        raise SystemExit('This release predates bundled helpers and guided setup; select a newer release.')
    if metadata.get('channel') != channel or metadata.get('version') != tag or metadata.get('target') != 'aarch64-unknown-linux-gnu':
        raise SystemExit('Release metadata does not match requested channel/version/architecture')
    bundle.extractall(directory)
print(f'Downloaded {tag}; verified {channel} ARM64 bundle.')
PY
cd "$download_dir/frame-voice"
sha256sum -c FILES-SHA256SUMS
./install.sh ./bin/frame-voice --tray-binary ./bin/frame-voice-tray --setup --enable-tray "$@"
