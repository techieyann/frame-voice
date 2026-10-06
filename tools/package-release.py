#!/usr/bin/env python3
"""Package only the files required to install a native ARM64 release."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile

PROJECT = Path(__file__).resolve().parent.parent
TARGET = 'aarch64-unknown-linux-gnu'
TAG = re.compile(r'^v((?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*))(?:-experimental\.([1-9]\d*))?$')


def classify(tag):
    match = TAG.fullmatch(tag)
    if not match:
        raise ValueError('Use vX.Y.Z or vX.Y.Z-experimental.N')
    return match[1], 'experimental' if match[2] else 'stable'


def version(manifest):
    package = manifest.read_text().split('[package]', 1)[1].split('\n[', 1)[0]
    return re.search(r'^version\s*=\s*"([^"]+)"', package, re.MULTILINE)[1]


def files(project, binaries, tray, sdk_license, runtime=None):
    runtime = runtime or project / '.build/runtime'
    source = {f'bin/{name}': binaries / name for name in ('frame-voice', 'frame-voice-paste')}
    source['bin/frame-voice-tray'] = tray
    for name in ('install.sh', 'install-gui.sh', 'rollback.sh', 'uninstall.sh'):
        source[name] = project / name
    for name in ('local-install.py', 'tray-session.py', 'config-gui.py', 'config-gui.sh', 'config-gui-zenity.sh', 'setup-backend.py', 'setup-interface.py', 'mic-gain.sh'):
        source['tools/' + name] = project / 'tools' / name
    for name in ('actions.json', 'frame_controller_binding.json', 'groq-prompt.txt'):
        source['assets/' + name] = project / 'assets' / name
    for name in ('microphone.svg', 'cancelled-microphone.svg', 'gear.svg'):
        source['assets/icons/' + name] = project / 'assets/icons' / name
    for name in ('frame-voice.service', 'frame-voice-tray-session.service', 'frame-voice-input.service', 'frame-voice-mixer.service'):
        source['systemd/' + name] = project / 'systemd' / name
    source['licenses/OpenVR.txt'] = sdk_license
    source['licenses/Frame-Voice.txt'] = project / 'LICENSE'
    for name in ('ydotool', 'ydotoold', 'whisper-cli', 'ffmpeg'):
        source['bin/' + name] = runtime / 'bin' / name
    for name in ('ydotool.txt', 'Whisper.txt', 'FFmpeg.txt'):
        source['licenses/' + name] = runtime / 'licenses' / name
    for name in ('ydotool-v1.0.4.tar.gz', 'whisper-v1.9.4.tar.gz', 'ffmpeg-n8.0.3.tar.gz', 'build-runtime.sh'):
        source['sources/' + name] = runtime / 'sources' / name
    for name, path in source.items():
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'Missing regular release file: {name}: {path}')
    return {name: (path.read_bytes(), 0o755 if name.startswith('bin/') or path.suffix == '.sh' else 0o644)
            for name, path in source.items()}


def pack(output, tag, commit, contents):
    _, channel = classify(tag)
    contents = dict(contents)
    contents['release.json'] = (json.dumps({'installer_version': 2, 'version': tag, 'channel': channel, 'target': TARGET,
                                           'commit': commit}, indent=2).encode() + b'\n', 0o644)
    checksums = ''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n'
                        for name, (data, _) in sorted(contents.items()))
    contents['FILES-SHA256SUMS'] = (checksums.encode(), 0o644)
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f'frame-voice-{tag}-{TARGET}.tar.gz'
    with archive.open('wb') as raw, gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as bundle:
            for name, (data, mode) in sorted(contents.items()):
                info = tarfile.TarInfo('frame-voice/' + name)
                info.size, info.mode = len(data), mode
                bundle.addfile(info, io.BytesIO(data))
    (output / 'SHA256SUMS').write_text(f'{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n')
    return archive


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tag', required=True)
    parser.add_argument('--binaries', type=Path, default=PROJECT / 'target/release')
    parser.add_argument('--tray', type=Path, default=PROJECT / 'tray/target/release/frame-voice-tray')
    parser.add_argument('--openvr-license', required=True, type=Path)
    parser.add_argument('--output', type=Path, default=PROJECT / 'dist')
    parser.add_argument('--runtime', type=Path, default=PROJECT / '.build/runtime')
    args = parser.parse_args()
    base, channel = classify(args.tag)
    if any(version(path) != base for path in (PROJECT / 'Cargo.toml', PROJECT / 'tray/Cargo.toml')):
        raise SystemExit('Tag base version must match both Cargo.toml package versions')
    for executable, expected in ((args.binaries / 'frame-voice', 'OpenVR compiled=true'),
                                 (args.tray, 'StatusNotifier compiled=true')):
        result = subprocess.run([str(executable), '--build-info'], capture_output=True, text=True, timeout=10)
        if result.returncode or expected not in result.stdout:
            raise SystemExit(f'Invalid release executable: {executable}')
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=PROJECT, text=True).strip()
    archive = pack(args.output, args.tag, commit, files(PROJECT, args.binaries, args.tray, args.openvr_license, args.runtime))
    print(f'{channel}: {archive}')
