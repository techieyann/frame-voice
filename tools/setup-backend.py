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
    'local-fast': 'Local — Fast (offline; 142 MiB download)',
    'local-balanced': 'Local — Balanced (more detail, slower; 466 MiB download)',
}
# Language names/codes from whisper.cpp v1.9.4 (base/small support 99 languages).
LANGUAGES = {'af': 'Afrikaans',
 'sq': 'Albanian',
 'am': 'Amharic',
 'ar': 'Arabic',
 'hy': 'Armenian',
 'as': 'Assamese',
 'az': 'Azerbaijani',
 'ba': 'Bashkir',
 'eu': 'Basque',
 'be': 'Belarusian',
 'bn': 'Bengali',
 'bs': 'Bosnian',
 'br': 'Breton',
 'bg': 'Bulgarian',
 'ca': 'Catalan',
 'zh': 'Chinese',
 'hr': 'Croatian',
 'cs': 'Czech',
 'da': 'Danish',
 'nl': 'Dutch',
 'en': 'English',
 'et': 'Estonian',
 'fo': 'Faroese',
 'fi': 'Finnish',
 'fr': 'French',
 'gl': 'Galician',
 'ka': 'Georgian',
 'de': 'German',
 'el': 'Greek',
 'gu': 'Gujarati',
 'ht': 'Haitian Creole',
 'ha': 'Hausa',
 'haw': 'Hawaiian',
 'he': 'Hebrew',
 'hi': 'Hindi',
 'hu': 'Hungarian',
 'is': 'Icelandic',
 'id': 'Indonesian',
 'it': 'Italian',
 'ja': 'Japanese',
 'jw': 'Javanese',
 'kn': 'Kannada',
 'kk': 'Kazakh',
 'km': 'Khmer',
 'ko': 'Korean',
 'lo': 'Lao',
 'la': 'Latin',
 'lv': 'Latvian',
 'ln': 'Lingala',
 'lt': 'Lithuanian',
 'lb': 'Luxembourgish',
 'mk': 'Macedonian',
 'mg': 'Malagasy',
 'ms': 'Malay',
 'ml': 'Malayalam',
 'mt': 'Maltese',
 'mi': 'Maori',
 'mr': 'Marathi',
 'mn': 'Mongolian',
 'my': 'Myanmar',
 'ne': 'Nepali',
 'no': 'Norwegian',
 'nn': 'Nynorsk',
 'oc': 'Occitan',
 'ps': 'Pashto',
 'fa': 'Persian',
 'pl': 'Polish',
 'pt': 'Portuguese',
 'pa': 'Punjabi',
 'ro': 'Romanian',
 'ru': 'Russian',
 'sa': 'Sanskrit',
 'sr': 'Serbian',
 'sn': 'Shona',
 'sd': 'Sindhi',
 'si': 'Sinhala',
 'sk': 'Slovak',
 'sl': 'Slovenian',
 'so': 'Somali',
 'es': 'Spanish',
 'su': 'Sundanese',
 'sw': 'Swahili',
 'sv': 'Swedish',
 'tl': 'Tagalog',
 'tg': 'Tajik',
 'ta': 'Tamil',
 'tt': 'Tatar',
 'te': 'Telugu',
 'th': 'Thai',
 'bo': 'Tibetan',
 'tr': 'Turkish',
 'tk': 'Turkmen',
 'uk': 'Ukrainian',
 'ur': 'Urdu',
 'uz': 'Uzbek',
 'vi': 'Vietnamese',
 'cy': 'Welsh',
 'yi': 'Yiddish',
 'yo': 'Yoruba'}

# Checksums: https://github.com/ggml-org/whisper.cpp/blob/v1.9.4/models/README.md
MODELS = {
    'local-fast': ('base.en', '137c40403d78fd54d454da0f9bd998f78703390c'),
    'local-balanced': ('small.en', 'db8a495a91d927739e50b3fc1cc4c6b8f6c2d022'),
    'local-fast-multilingual': ('base', '465707469ff3a37a2b9b8d8f89f2f99de7299dac'),
    'local-balanced-multilingual': ('small', '55356645c2b361a969dfd0ef2c5a50d530afd8d5'),
}


SETTINGS = {
    'general': [
        ('VOICE_INJECT', 'Text delivery', 'enum', ['type', 'paste'], 'type'),
        ('VOICE_BEEP', 'Start and stop tones', 'bool', None, True),
        ('VOICE_TRAILING', 'Separator after dictation', 'enum', ['space', 'none'], 'space'),
        ('VOICE_MAX', 'Maximum recording seconds', 'text', None, '30'),
    ],
    'controllers': [
        ('VRBTN_DICTATE_RIGHT', 'Right hand dictation', 'bool', None, True),
        ('VRBTN_SUBMIT_RIGHT', 'Right submit button', 'enum', ['a', 'b', 'x', 'y', 'none'], 'a'),
        ('VRBTN_CLEAR_RIGHT', 'Right clear button', 'enum', ['a', 'b', 'x', 'y', 'none'], 'b'),
        ('VRBTN_CLEAR_B_SPACE', 'B also types a space in Desktop', 'bool', None, True),
        ('VRBTN_DICTATE_LEFT', 'Left hand dictation', 'bool', None, True),
        ('VRBTN_SUBMIT_LEFT', 'Left submit button', 'enum', ['dpad_up', 'dpad_down', 'dpad_left', 'dpad_right', 'none'], 'dpad_right'),
        ('VRBTN_CLEAR_LEFT', 'Left clear button', 'enum', ['dpad_up', 'dpad_down', 'dpad_left', 'dpad_right', 'none'], 'dpad_left'),
    ],
}

def option_label(value):
    labels = {'dpad_up': 'D-pad Up', 'dpad_down': 'D-pad Down',
              'dpad_left': 'D-pad Left', 'dpad_right': 'D-pad Right', 'none': 'None'}
    return labels.get(value, value.upper() if value in ('a', 'b', 'x', 'y') else value.capitalize())


def write_private_text(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', dir=path.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(content)
        temporary.chmod(0o600)
        os.replace(temporary, path)
        temporary = None
    finally:
        if temporary is not None: temporary.unlink(missing_ok=True)


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
    print(f'Downloading {model_label(mode)}…', flush=True)
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


def normalize_language(value):
    code = value.strip().lower() or 'auto'
    if code != 'auto' and code not in LANGUAGES:
        raise RuntimeError('Choose a supported transcription language or automatic detection')
    return code


def model_label(mode):
    speed = 'Fast' if mode.startswith('local-fast') else 'Balanced'
    variant = 'Multilingual' if mode.endswith('-multilingual') else 'English'
    return f'Local · {speed} · {variant}'


def downloaded_models(home):
    directory = home/'.local/share/frame-voice/models'
    for path in (directory, *directory.parents):
        if path == home: break
        if path.is_symlink(): raise RuntimeError('Model directory must not be a symlink')
    return [(mode, directory/f'ggml-{model}.bin')
            for mode, (model, _) in MODELS.items()
            if (directory/f'ggml-{model}.bin').is_file()
            and not (directory/f'ggml-{model}.bin').is_symlink()]


def model_in_use(home, path):
    values = read_values(home)
    current = values.get('VOICE_MODEL', '')
    return values.get('VOICE_BACKEND', 'groq') == 'local' and bool(current) and Path(current).expanduser().resolve() == path.resolve()


def delete_model(home, mode):
    models = dict(downloaded_models(home))
    if mode not in models: raise RuntimeError('Downloaded model is no longer available')
    path = models[mode]
    if model_in_use(home, path):
        raise RuntimeError('This model is in use. Save another transcription path before deleting it.')
    path.unlink()


def input_permissions(uid, gui=False):
    if os.access('/dev/uinput', os.W_OK): return
    # A one-time device rule grants input access to this Frame user; the helper
    # itself remains unprivileged. No root service executes user-writable code.
    rule = f'KERNEL=="uinput", SUBSYSTEM=="misc", OWNER="{uid}", MODE="0600"\n'
    print('Input setup needs your SteamOS administrator password once. If you have not set one, run passwd first.', flush=True)
    auth = ['/usr/bin/pkexec'] if gui else ['sudo']
    with tempfile.TemporaryDirectory(prefix='frame-uinput-') as directory:
        source = Path(directory)/'99-frame-voice-uinput.rules'
        source.write_text(rule)
        subprocess.run([*auth, '/usr/bin/install', '-m', '644', str(source), '/etc/udev/rules.d/99-frame-voice-uinput.rules'], check=True)
        module = Path(directory)/'frame-voice.conf'
        module.write_text('uinput\n')
        subprocess.run([*auth, '/usr/bin/install', '-m', '644', str(module), '/etc/modules-load.d/frame-voice.conf'], check=True)
    subprocess.run([*auth, '/usr/bin/modprobe', 'uinput'], check=True)
    subprocess.run([*auth, '/usr/bin/udevadm', 'control', '--reload-rules'], check=True)
    subprocess.run([*auth, '/usr/bin/udevadm', 'trigger', '--action=change', '--subsystem-match=misc', '--sysname-match=uinput'], check=True)
    subprocess.run([*auth, '/usr/bin/udevadm', 'settle'], check=True)
    if not os.access('/dev/uinput', os.W_OK): raise RuntimeError('Input permissions are not ready; backend config remains unchanged')


def prepare(home, requested=None, key_file=None, api_key=None, allow_prompt=True, language=None):
    values = read_values(home)
    language = normalize_language(values.get('VOICE_LANG', 'en') if language is None else language)
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
        if requested is None and not allow_prompt:
            raise RuntimeError('Provide --backend groq, local-fast, or local-balanced')
        mode = requested or choose()
    if mode not in (*MODES, 'existing-local'): raise RuntimeError('Invalid backend')
    updates = {'VOICE_LANG': language, 'YDOTOOL_BIN': values.get('YDOTOOL_BIN') or str(helpers/'ydotool'),
               'FFMPEG_BIN': values.get('FFMPEG_BIN') or str(helpers/'ffmpeg'),
               'YDOTOOL_SOCKET': values.get('YDOTOOL_SOCKET') or f'/run/user/{os.getuid()}/frame-voice-ydotool.sock'}
    if mode == 'groq':
        key = key_file.read_text().strip() if key_file else api_key or existing_key
        if not key and allow_prompt: key = key_prompt()
        if not key or any(character.isspace() for character in key): raise RuntimeError('A valid nonempty Groq API key is required')
        updates.update(VOICE_BACKEND='groq', GROQ_API_KEY=key)
    else:
        if not (helpers/'whisper-cli').is_file(): raise RuntimeError('Bundled Whisper engine is missing')
        if mode != 'existing-local':
            model_mode = mode if language == 'en' else mode + '-multilingual'
            model = download_model(home, model_mode)
            updates.update(VOICE_BACKEND='local', VOICE_MODEL=str(model), VOICE_CLEANUP='0', VOICE_THREADS='4')
        elif language != 'en' and values.get('VOICE_MODEL', '').endswith('.en.bin'):
            raise RuntimeError('The current model is English-only. Choose Local Fast or Local Balanced for other languages.')
        updates['WHISPER_BIN'] = values.get('WHISPER_BIN') or str(helpers/'whisper-cli')
    return updates, mode


def configure(home, requested=None, key_file=None, allow_permissions=True, language=None):
    updates, mode = prepare(home, requested, key_file, language=language)
    if allow_permissions: input_permissions(os.getuid())
    write_values(home/'.config/frame-voice/env', updates)
    print(f'Configured {MODES.get(mode, "Local Whisper")}.', flush=True)


def manage_models(home):
    models = [(mode, path) for mode, path in downloaded_models(home) if not model_in_use(home, path)]
    if not models:
        subprocess.run(['zenity', '--info', '--title=Downloaded models',
                        '--text=No unused downloads. Save another transcription path before deleting the active model.'], check=False)
        return
    command = ['zenity', '--list', '--radiolist', '--title=Downloaded models',
               '--text=Choose an unused model to delete.', '--column=Use', '--column=Mode',
               '--column=Model', '--hide-column=2', '--print-column=2']
    for index, (mode, path) in enumerate(models):
        command.extend(['TRUE' if index == 0 else 'FALSE', mode,
                        f'{model_label(mode)} · {path.stat().st_size/(1024*1024):.1f} MiB'])
    selected = subprocess.run(command, capture_output=True, text=True)
    if selected.returncode: return
    confirmed = subprocess.run(['zenity', '--question', '--title=Delete model?',
                                '--text=Remove this download? Your settings and API key will be kept. You can download the model again later.',
                                '--ok-label=Delete model', '--cancel-label=Cancel', '--default-cancel'])
    if confirmed.returncode == 0: delete_model(home, selected.stdout.strip())


def groq_preferences(home):
    if not (os.environ.get('DISPLAY') or os.environ.get('WAYLAND_DISPLAY')): return {}
    values = read_values(home)
    specs = [('VOICE_CLEANUP', 'Clean up transcript', True),
             ('GROQ_USAGE_NOTIFY', 'In-headset notifications', True),
             ('VOICE_SLASH', 'Convert spoken slash to /', False)]
    command = ['zenity', '--list', '--checklist', '--title=Groq settings',
               '--text=Choose cloud transcription options.', '--separator=|',
               '--column=Use', '--column=Option']
    for key, label, default in specs:
        enabled = values.get(key, '1' if default else '0').lower() in ('1','true','yes','on')
        command += ['TRUE' if enabled else 'FALSE', label]
    command += ['FALSE', 'Edit cleanup prompt…']
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode: raise RuntimeError('Groq setup canceled')
    selected = result.stdout.strip().split('|')
    if 'Edit cleanup prompt…' in selected:
        path = home/'.config/frame-voice/groq-prompt.txt'
        if not path.exists():
            template = Path(__file__).with_name('groq-prompt.txt')
            if not template.exists(): template = Path(__file__).parent.parent/'assets/groq-prompt.txt'
            write_private_text(path, template.read_text())
        subprocess.Popen(['xdg-open', str(path)])
    return {key: '1' if label in selected else '0' for key, label, _ in specs}


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('backend', choices=(*MODES, 'manage-models'))
    parser.add_argument('--language', default=None, help='Whisper language code or auto')
    args = parser.parse_args()
    try:
        home = Path.home()
        if args.backend == 'manage-models':
            manage_models(home)
            raise SystemExit(0)
        key = key_prompt() if args.backend == 'groq' else None
        preferences = groq_preferences(home) if args.backend == 'groq' else {}
        updates, mode = prepare(home, args.backend, api_key=key, allow_prompt=False, language=args.language)
        updates.update(preferences)
        write_values(home/'.config/frame-voice/env', updates)
    except (OSError, ValueError, RuntimeError) as error:
        raise SystemExit(str(error))
