#!/usr/bin/env python3
"""First-run backend selection and verified local-model installation."""
import getpass
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import urllib.request

MODES = {
    'groq': 'Groq — Cloud (internet and API key required)',
    'local-fast': 'Local — Fast (English, offline; 142 MiB download)',
    'local-balanced': 'Local — Balanced (English, more detail, slower; 466 MiB download)',
}
# Checksums published by whisper.cpp/models/README.md for the English models.
MODELS = {
    'local-fast': ('base.en', '137c40403d78fd54d454da0f9bd998f78703390c'),
    'local-balanced': ('small.en', 'db8a495a91d927739e50b3fc1cc4c6b8f6c2d022'),
}


def read_values(home):
    values = {}
    for path in (home/'.config/voice-prompt/env', home/'.config/frame-voice/env'):
        if path.is_file():
            for line in path.read_text().splitlines():
                if line.strip() and not line.lstrip().startswith('#') and '=' in line:
                    key, value = line.split('=', 1)
                    values[key.strip()] = value.strip().strip('\"\'')
    return values


def write_values(path, updates):
    lines = path.read_text().splitlines() if path.exists() else []
    pending = dict(updates)
    result = []
    for line in lines:
        key = line.split('=', 1)[0].strip() if '=' in line and not line.lstrip().startswith('#') else None
        if key in updates:
            if key in pending:
                result.append(f'{key}={pending.pop(key)}')
        else:
            result.append(line)
    result.extend(f'{key}={value}' for key, value in pending.items())
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write('\n'.join(result)+'\n')
        temporary.chmod(0o600)
        os.replace(temporary, path)
        temporary = None
    finally:
        if temporary is not None: temporary.unlink(missing_ok=True)


def choose():
    if shutil.which('zenity') and (os.environ.get('DISPLAY') or os.environ.get('WAYLAND_DISPLAY')):
        arguments = ['zenity', '--list', '--radiolist', '--title=Frame Voice setup',
                     '--text=Choose how speech is transcribed', '--column=Use', '--column=Mode',
                     '--column=Description', '--hide-column=2', '--print-column=2']
        for index, (mode, label) in enumerate(MODES.items()):
            arguments.extend(['TRUE' if index == 0 else 'FALSE', mode, label])
        selected = subprocess.run(arguments, capture_output=True, text=True)
        if selected.returncode: raise RuntimeError('Backend setup canceled; installed files retained')
        mode = selected.stdout.strip()
    else:
        try:
            with open('/dev/tty', 'r+') as terminal:
                for index, label in enumerate(MODES.values(), 1): terminal.write(f'{index}. {label}\n')
                terminal.write('Choose 1, 2 or 3: ')
                terminal.flush()
                answer = terminal.readline().strip()
                if answer not in ('1', '2', '3'): raise RuntimeError('Select a valid backend')
                mode = list(MODES)[int(answer)-1]
        except OSError as error:
            raise RuntimeError('Interactive setup needs a terminal or Desktop; use --backend groq, local-fast or local-balanced') from error
    if mode not in MODES: raise RuntimeError('Invalid backend choice')
    return mode


def key_prompt():
    if shutil.which('zenity') and (os.environ.get('DISPLAY') or os.environ.get('WAYLAND_DISPLAY')):
        result = subprocess.run(['zenity', '--entry', '--hide-text', '--title=Groq API key',
                                 '--text=Create an API key at https://console.groq.com/keys, then paste it here.'],
                                capture_output=True, text=True)
        if result.returncode: raise RuntimeError('API key setup canceled; installed files retained')
        key = result.stdout.strip()
    else:
        try:
            with open('/dev/tty', 'r+') as terminal:
                terminal.write('Create a key at https://console.groq.com/keys\n')
                terminal.flush()
                key = getpass.getpass('Groq API key: ', stream=terminal).strip()
        except OSError as error: raise RuntimeError('Provide a key with --groq-key-file for noninteractive setup') from error
    return key


def checksum(path):
    digest = hashlib.sha1()
    with path.open('rb') as source:
        for chunk in iter(lambda: source.read(1024*1024), b''): digest.update(chunk)
    return digest.hexdigest()


def download_model(home, mode, opener=urllib.request.urlopen):
    model, expected = MODELS[mode]
    directory = home/'.local/share/frame-voice/models'
    directory.mkdir(parents=True, exist_ok=True)
    destination = directory/f'ggml-{model}.bin'
    if destination.is_file() and checksum(destination) == expected: return destination
    url = f'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{model}.bin'
    temporary = None
    print(f'Downloading {MODES[mode]}…', flush=True)
    try:
        digest = hashlib.sha1()
        with tempfile.NamedTemporaryFile(dir=directory, delete=False) as stream:
            temporary = Path(stream.name)
            with opener(url, timeout=60) as response:
                for chunk in iter(lambda: response.read(1024*1024), b''):
                    digest.update(chunk)
                    stream.write(chunk)
        if digest.hexdigest() != expected: raise RuntimeError('Model checksum mismatch; existing model retained')
        temporary.chmod(0o600)
        os.replace(temporary, destination)
        temporary = None
    finally:
        if temporary is not None: temporary.unlink(missing_ok=True)
    return destination


def input_permissions(uid):
    if os.access('/dev/uinput', os.W_OK): return
    # A one-time device rule grants input access to this Frame user; the helper
    # itself remains unprivileged. No root service executes user-writable code.
    rule = f'KERNEL=="uinput", SUBSYSTEM=="misc", OWNER="{uid}", MODE="0600"\n'
    print('Input setup needs your SteamOS administrator password once. If you have not set one, run passwd first.', flush=True)
    with tempfile.TemporaryDirectory(prefix='frame-uinput-') as directory:
        source = Path(directory)/'99-frame-voice-uinput.rules'
        source.write_text(rule)
        subprocess.run(['sudo', 'install', '-m', '644', str(source), '/etc/udev/rules.d/99-frame-voice-uinput.rules'], check=True)
        module = Path(directory)/'frame-voice.conf'
        module.write_text('uinput\n')
        subprocess.run(['sudo', 'install', '-m', '644', str(module), '/etc/modules-load.d/frame-voice.conf'], check=True)
    subprocess.run(['sudo', 'modprobe', 'uinput'], check=True)
    subprocess.run(['sudo', 'udevadm', 'control', '--reload-rules'], check=True)
    subprocess.run(['sudo', 'udevadm', 'trigger', '--action=change', '--subsystem-match=misc', '--sysname-match=uinput'], check=True)
    subprocess.run(['sudo', 'udevadm', 'settle'], check=True)
    if not os.access('/dev/uinput', os.W_OK): raise RuntimeError('Input permissions are not ready; backend config remains unchanged')


def configure(home, requested=None, key_file=None, allow_permissions=True):
    values = read_values(home)
    helpers = home/'.local/share/frame-voice/bin'
    for name in ('ydotool', 'ydotoold', 'ffmpeg'):
        if not (helpers/name).is_file(): raise RuntimeError(f'Bundled helper is missing: {name}')
    current = values.get('VOICE_BACKEND', 'groq')
    existing_key = values.get('GROQ_API_KEY', '').strip()
    if not existing_key:
        for candidate in (home/'.config/frame-voice/groq.key', home/'.config/voice-prompt/groq.key', home/'.local/share/voice-prompt/groq.key'):
            if candidate.is_file(): existing_key = candidate.read_text().strip(); break
    if requested is None and current == 'groq' and existing_key: requested = 'groq'
    if requested is None and current == 'local' and Path(values.get('VOICE_MODEL', '/nonexistent')).is_file():
        mode = 'existing-local'
    else:
        mode = requested or choose()
    if mode not in (*MODES, 'existing-local'): raise RuntimeError('Invalid backend')
    updates = {'YDOTOOL_BIN': values.get('YDOTOOL_BIN') or str(helpers/'ydotool'),
               'FFMPEG_BIN': values.get('FFMPEG_BIN') or str(helpers/'ffmpeg'),
               'YDOTOOL_SOCKET': values.get('YDOTOOL_SOCKET') or f'/run/user/{os.getuid()}/frame-voice-ydotool.sock'}
    if mode == 'groq':
        key = key_file.read_text().strip() if key_file else existing_key or key_prompt()
        if not key or any(character.isspace() for character in key): raise RuntimeError('A valid nonempty Groq API key is required')
        updates.update(VOICE_BACKEND='groq', GROQ_API_KEY=key)
    else:
        if not (helpers/'whisper-cli').is_file(): raise RuntimeError('Bundled Whisper engine is missing')
        if mode != 'existing-local':
            model = download_model(home, mode)
            updates.update(VOICE_BACKEND='local', VOICE_MODEL=str(model), VOICE_CLEANUP='0', VOICE_THREADS='4', VOICE_LANG='en')
        updates['WHISPER_BIN'] = values.get('WHISPER_BIN') or str(helpers/'whisper-cli')
    if allow_permissions: input_permissions(os.getuid())
    write_values(home/'.config/frame-voice/env', updates)
    print(f'Configured {MODES.get(mode, "Local Whisper")}.', flush=True)
