#!/usr/bin/env python3
"""Transactional Frame installation with optional guided backend setup."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import time

PROJECT = Path(__file__).resolve().parent.parent
UNITS = ('frame-voice.service', 'frame-voice-tray-session.service', 'frame-voice-input.service', 'frame-voice-mixer.service')
CONFIG = '.config/frame-voice/env'
EXAMPLE = b'''# Frame Voice configuration.
# Literal KEY=VALUE (not shell). Uncomment a line to change it. Keep mode 600.
#
# == Groq API key  (required for the default backend) ==
# Create a key at https://console.groq.com/keys
GROQ_API_KEY=
# Or put just the key in ~/.config/frame-voice/groq.key (mode 600).
#
# == Backend ==
# VOICE_BACKEND=groq                 # groq (default) or local (whisper-cli)
#
# == Dictation ==
# VOICE_CLEANUP=1
# VOICE_LANG=en
# VOICE_TRAILING=space               # space | none
# VOICE_SLASH=1                      # render spoken "slash" as /
# VOICE_MIN_SPEECH_MS=200
# VOICE_MAX=30
#
# == Injection ==
# VOICE_INJECT=paste                 # paste (clipboard) or type
# VOICE_PASTE_CHUNK=0                # 0 = one paste; >0 = max chars per paste
# YDOTOOL_KEY_DELAY=2
# YDOTOOL_KEY_HOLD=2
#
# == Feedback ==
# VOICE_BEEP=1
# VOICE_NOTIFY=1
# VRBTN_GROW_MS=450
# VRBTN_BEEP_LEAD_MS=150
#
# == Gesture timing ==
# VRBTN_ARM_MS=150
# VRBTN_TAP_MS=250
# VRBTN_READY_MS=600
# VOICE_ACTION_WINDOW_MS=10000
#
# == Usage warnings ==
# VOICE_WARN_REQUESTS=100
# VOICE_WARN_TOKENS=1000
# VOICE_WARN_COOLDOWN_SEC=3600
#
# == Local ASR  (VOICE_BACKEND=local) ==
# VOICE_MODEL=/home/steamos/.local/share/whisper/models/ggml-base.en.bin
# WHISPER_BIN=whisper-cli
# VOICE_THREADS=4
'''


def digest(data):
    return hashlib.sha256(data).hexdigest()


def atomic_write(path, data, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix='.frame-install-', delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        temporary.chmod(mode)
        os.replace(temporary, path)
        temporary = None
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def managed_path(home, name):
    relative = Path(name)
    if relative.is_absolute() or '..' in relative.parts or not relative.parts or relative.parts[0] not in ('.local', '.config'):
        raise RuntimeError('Invalid installation manifest path')
    path = home / relative
    if path.is_symlink() or any(parent.is_symlink() for parent in path.parents if parent != home and home in parent.parents):
        raise RuntimeError(f'Refusing managed symlink: {path}')
    return path


def payload(project, binary, tray, include_tray, home):
    files = {}
    def add(source, target, mode=0o644):
        if source.is_symlink() or not source.is_file():
            raise RuntimeError(f'Missing regular source file: {source}')
        files[target] = (source.read_bytes(), mode)
    add(binary, '.local/bin/frame-voice', 0o755)
    add(binary.parent / 'frame-voice-paste', '.local/bin/frame-voice-paste', 0o755)
    for name in ('actions.json', 'frame_controller_binding.json'):
        add(project / 'assets' / name, '.local/share/frame-voice/' + name)
    add(project / 'systemd/frame-voice.service', '.config/systemd/user/frame-voice.service')
    for name in ('ydotool', 'ydotoold', 'whisper-cli', 'ffmpeg'):
        source = project/'bin'/name
        if not source.is_file(): source = project/'.build/runtime/bin'/name
        if source.is_file(): add(source, '.local/share/frame-voice/bin/'+name, 0o755)
    for name in ('frame-voice-input.service', 'frame-voice-mixer.service'):
        add(project/'systemd'/name, '.config/systemd/user/'+name)
    add(project/'tools/mic-gain.sh', '.local/share/frame-voice/mic-gain.sh', 0o755)
    if (project / 'release.json').is_file():
        add(project / 'release.json', '.local/share/frame-voice/release.json')
    for source in (project/'licenses').glob('*.txt'):
        add(source, '.local/share/frame-voice/licenses/'+source.name)
    for source in (project/'.build/runtime/licenses').glob('*.txt'):
        add(source, '.local/share/frame-voice/licenses/'+source.name)
    if include_tray:
        add(tray, '.local/bin/frame-voice-tray', 0o755)
        add(project / 'tools/tray-session.py', '.local/share/frame-voice/tray-session.py', 0o755)
        add(project / 'tools/config-gui.sh', '.local/share/frame-voice/config-gui.sh', 0o755)
        add(project / 'tools/config-gui.py', '.local/share/frame-voice/config-gui.py', 0o755)
        add(project / 'tools/config-gui-zenity.sh', '.local/share/frame-voice/config-gui-zenity.sh', 0o755)
        add(project / 'systemd/frame-voice-tray-session.service', '.config/systemd/user/frame-voice-tray-session.service')
        for name, state in [('microphone.svg', 'active'), ('cancelled-microphone.svg', 'stopped'),
                            ('gear.svg', 'processing'), ('cancelled-microphone.svg', 'failed')]:
            add(project / 'assets/icons' / name, f'.local/share/frame-voice/tray-icons/frame-voice-{state}.svg')
    if not (home / CONFIG).exists():
        files[CONFIG] = (EXAMPLE, 0o600)
    for name in files:
        managed_path(home, name)
    return files


class Transaction:
    def __init__(self, home):
        self.home = home
        self.root = home / '.local/share/frame-voice-installer'

    def install(self, files, service_states):
        self.root.mkdir(parents=True, exist_ok=True, mode=0o700)
        snapshot = self.root / 'transactions' / str(time.time_ns())
        snapshot.mkdir(parents=True, mode=0o700)
        entries = []
        # Snapshot everything before touching any destination.
        for index, (name, (data, mode)) in enumerate(files.items()):
            target = managed_path(self.home, name)
            if target.exists() and not target.is_file():
                raise RuntimeError(f'Not a regular target: {target}')
            existed = target.exists()
            if existed:
                shutil.copy2(target, snapshot / str(index))
                (snapshot / str(index)).chmod(0o600)
            entries.append({'path': name, 'backup': str(index), 'existed': existed,
                            'mode': target.stat().st_mode & 0o777 if existed else mode,
                            'installed_sha256': digest(data)})
        manifest = {'entries': entries, 'services': service_states}
        atomic_write(snapshot / 'manifest.json', json.dumps(manifest, indent=2).encode(), 0o600)
        try:
            for name, (data, mode) in files.items():
                atomic_write(managed_path(self.home, name), data, mode)
        except BaseException:
            self.restore(snapshot, preserve_modified=False)
            raise
        return snapshot

    def restore(self, snapshot, preserve_modified=True):
        manifest = json.loads((snapshot / 'manifest.json').read_text())
        for entry in reversed(manifest['entries']):
            target = managed_path(self.home, entry['path'])
            if preserve_modified and target.exists() and digest(target.read_bytes()) != entry['installed_sha256']:
                print(f'Preserved modified file: {target}')
                continue
            if entry['existed']:
                atomic_write(target, (snapshot / entry['backup']).read_bytes(), entry['mode'])
            else:
                target.unlink(missing_ok=True)
        return manifest

    def current(self):
        name = (self.root / 'current').read_text().strip()
        if not name.isdigit():
            raise RuntimeError('Invalid snapshot pointer')
        return self.root / 'transactions' / name

    def check_restore(self, snapshot):
        manifest = json.loads((snapshot / 'manifest.json').read_text())
        for entry in manifest['entries']:
            target = managed_path(self.home, entry['path'])
            if entry['path'] != CONFIG and target.exists() and digest(target.read_bytes()) != entry['installed_sha256']:
                raise RuntimeError(f'Rollback would overwrite a newer/modified file: {target}')

    def commit(self, snapshot):
        atomic_write(self.root / 'current', snapshot.name.encode(), 0o600)


class Services:
    def __init__(self, uid):
        self.env = dict(os.environ, XDG_RUNTIME_DIR=f'/run/user/{uid}',
                        DBUS_SESSION_BUS_ADDRESS=f'unix:path=/run/user/{uid}/bus')

    def call(self, *args, required=True):
        result = subprocess.run(['systemctl', '--user', '--no-pager', *args], env=self.env,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
        if required and result.returncode:
            raise RuntimeError(f'systemctl {args[0]} failed: {result.stderr.decode(errors="replace").strip()}')
        return result

    def states(self, units):
        return {unit: {'active': self.call('is-active', '--quiet', unit, required=False).returncode == 0,
                       'enabled': self.call('is-enabled', '--quiet', unit, required=False).returncode == 0}
                for unit in units}

    def restore(self, states):
        for unit, state in states.items():
            if state['enabled']:
                self.call('enable', unit)
            else:
                self.call('disable', unit, required=False)
            if state['active']:
                self.call('start', unit)


def capability(binary, expected):
    result = subprocess.run([str(binary), '--build-info'], capture_output=True, text=True, timeout=10)
    if result.returncode or expected not in result.stdout:
        raise RuntimeError(f'Wrong or unusable binary: {binary}; expected {expected}')


def execute(args):
    if platform.system() != 'Linux' or platform.machine() != 'aarch64' or os.getuid() == 0:
        raise RuntimeError('Run on the aarch64 SteamOS Frame as its normal user, without sudo')
    home = Path.home()
    transaction, services = Transaction(home), Services(os.getuid())
    if args.operation == 'install':
        binary = args.binary.resolve()
        tray = args.tray_binary.resolve()
        include_tray = not args.daemon_only
        if args.enable_tray and not include_tray:
            raise RuntimeError('--enable-tray cannot be combined with --daemon-only')
        capability(binary, 'OpenVR compiled=true')
        if include_tray:
            capability(tray, 'StatusNotifier compiled=true')
        files = payload(PROJECT, binary, tray, include_tray, home)
        previous = home / '.local/bin/frame-voice'
        if previous.is_file():
            files['.local/bin/frame-voice.previous'] = (previous.read_bytes(), previous.stat().st_mode & 0o777)
        units = UNITS if include_tray else (UNITS[0], UNITS[2], UNITS[3])
        services.call('daemon-reload')
        states = services.states(units)
        setup = getattr(args, 'setup', False) or getattr(args, 'backend', None) is not None
        if (args.activate and not setup) or states[UNITS[0]]['active']:
            subprocess.run([str(binary), '--check'], check=True, timeout=10)
        snapshot = None
        try:
            for unit in units:
                if states[unit]['active']:
                    services.call('stop', unit)
            snapshot = transaction.install(files, states)
            services.call('daemon-reload')
            services.restore(states)
            if args.enable_tray:
                services.call('enable', '--now', UNITS[1])
            if args.activate and not setup:
                services.call('enable', '--now', UNITS[0])
            transaction.commit(snapshot)
        except BaseException:
            if snapshot is not None:
                for unit in units:
                    services.call('stop', unit, required=False)
                    services.call('disable', unit, required=False)
                transaction.restore(snapshot, preserve_modified=False)
                services.call('daemon-reload', required=False)
            services.restore(states)
            raise
        print(f'Installed. Full rollback snapshot: {snapshot}')
        if setup:
            spec = importlib.util.spec_from_file_location('backend_setup', PROJECT/'tools/setup-backend.py')
            backend = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(backend)
            backend.configure(home, getattr(args, 'backend', None), getattr(args, 'groq_key_file', None))
            subprocess.run([str(home/'.local/bin/frame-voice'), '--check'], check=True, timeout=10)
            services.call('start', UNITS[2], UNITS[3])
            if include_tray: services.call('enable', '--now', UNITS[1])
            services.call('enable', '--now', UNITS[0])
            services.call('restart', UNITS[0])
            print('Ready. Open Desktop for the tray and hold a controller to dictate.')
            return
        print('Existing service state restored; new installs remain stopped unless explicitly activated.')
        print('Configure ~/.config/frame-voice/env or groq.key (mode 600), then run frame-voice --check.')
        print('Activate: systemctl --user enable --now frame-voice frame-voice-tray-session')
    else:
        snapshot = transaction.current()
        if args.operation == 'rollback':
            transaction.check_restore(snapshot)
        manifest = json.loads((snapshot / 'manifest.json').read_text())
        for unit in manifest['services']:
            services.call('stop', unit, required=False)
            services.call('disable', unit, required=False)
        if args.operation == 'rollback':
            transaction.restore(snapshot)
            services.call('daemon-reload')
            services.restore(manifest['services'])
            print('Restored the last installation snapshot; modified files were preserved.')
        else:
            for entry in manifest['entries']:
                if entry['path'] == CONFIG:
                    continue
                if entry['path'].endswith('.previous'):
                    continue
                target = managed_path(home, entry['path'])
                if target.exists():
                    if digest(target.read_bytes()) == entry['installed_sha256']:
                        target.unlink()
                    else:
                        print(f'Preserved modified file: {target}')
            services.call('daemon-reload')
            print('Uninstalled managed files; configuration, backups, ydotoold and voice-mixer retained.')


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=('install', 'uninstall', 'rollback'))
    parser.add_argument('binary', nargs='?', type=Path, default=PROJECT / 'target/release/frame-voice')
    parser.add_argument('--tray-binary', type=Path, default=PROJECT / 'tray/target/release/frame-voice-tray')
    parser.add_argument('--daemon-only', action='store_true')
    parser.add_argument('--enable-tray', action='store_true')
    parser.add_argument('--activate', action='store_true')
    parser.add_argument('--setup', action='store_true', help='Choose a backend, download its model if local, and activate')
    parser.add_argument('--backend', choices=('groq', 'local-fast', 'local-balanced'))
    parser.add_argument('--groq-key-file', type=Path, help='Read the Groq key from a local file without placing it in command arguments')
    return parser.parse_args()


if __name__ == '__main__':
    try:
        args = arguments()
        if platform.system() != 'Linux' or platform.machine() != 'aarch64' or os.getuid() == 0:
            raise RuntimeError('Run on the aarch64 SteamOS Frame as its normal user, without sudo')
        state = Path.home() / '.local/share/frame-voice-installer'
        state.mkdir(parents=True, exist_ok=True, mode=0o700)
        with open(state / 'install.lock', 'a') as lock:
            os.chmod(lock.name, 0o600)
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            execute(args)
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(f'Installation failed: {error}')
