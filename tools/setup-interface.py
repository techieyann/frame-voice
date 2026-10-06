#!/usr/bin/env python3
"""Installer frontends sharing settings and backend preparation."""
import curses
import importlib.util
import os
from pathlib import Path
import subprocess


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).with_name(name+'.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def desktop_environment():
    environment = dict(os.environ, GSK_RENDERER='cairo')
    if environment.get('DISPLAY') or environment.get('WAYLAND_DISPLAY'):
        return environment
    found = load('tray-session').discover(os.getuid())
    if found:
        environment.update(found[1])
        return environment
    return None


def choose_screen(screen, title, choices, selected=0):
    selected = max(0, min(selected, len(choices)-1))
    while True:
        screen.erase()
        height, width = screen.getmaxyx()
        if height < 10 or width < 40: raise RuntimeError('Enlarge the terminal to at least 40 columns and 10 rows')
        screen.addnstr(1, 2, title, width-4, curses.A_BOLD)
        screen.addnstr(height-2, 2, '↑/↓ select · Enter edit/accept · Esc cancel', width-4)
        visible = height-5
        offset = max(0, min(selected-visible+1, len(choices)-visible))
        for row, choice in enumerate(choices[offset:offset+visible], 3):
            screen.addnstr(row, 2, choice, width-4, curses.A_REVERSE if row-3+offset == selected else 0)
        screen.refresh()
        key = screen.getch()
        if key == curses.KEY_UP: selected = (selected-1) % len(choices)
        elif key == curses.KEY_DOWN: selected = (selected+1) % len(choices)
        elif key == curses.KEY_HOME: selected = 0
        elif key == curses.KEY_END: selected = len(choices)-1
        elif key in (10, 13, curses.KEY_ENTER): return selected
        elif key == 27: raise RuntimeError('Setup canceled')


def entry(screen, title, initial='', secret=False):
    value = initial
    curses.curs_set(1)
    try:
        while True:
            screen.erase()
            height, width = screen.getmaxyx()
            screen.addnstr(1, 2, title, width-4, curses.A_BOLD)
            shown = '•'*len(value) if secret else value
            screen.addnstr(3, 2, shown[-(width-5):], width-4)
            screen.addnstr(height-2, 2, 'Enter accept · Esc cancel · Ctrl+U clear', width-4)
            screen.move(3, min(2+len(value), width-3))
            screen.refresh()
            key = screen.get_wch()
            if key in ('\n', '\r'): return value
            if key == '\x1b': raise RuntimeError('Setup canceled')
            if key == '\x15': value = ''
            elif key in ('\x7f', '\b', curses.KEY_BACKSPACE): value = value[:-1]
            elif isinstance(key, str) and key.isprintable(): value += key
    finally:
        curses.curs_set(0)


def tui_options(screen, backend, home, requested, language, key_file):
    curses.curs_set(0)
    values = backend.read_values(home)
    mode = requested or 'groq'
    if requested is None and values.get('VOICE_BACKEND') == 'local':
        mode = 'local-balanced' if 'small' in values.get('VOICE_MODEL', '') else 'local-fast'
    language = backend.normalize_language(language or values.get('VOICE_LANG', 'en'))
    options = {}
    specs = [spec for section in backend.SETTINGS.values() for spec in section]
    specs.append(('VOICE_CLEANUP', 'Clean up transcript', 'bool', None, True))
    specs.append(('GROQ_USAGE_NOTIFY', 'In-headset Groq notifications', 'bool', None, True))
    specs.append(('VOICE_SLASH', 'Convert spoken slash to /', 'bool', None, False))
    for spec in specs:
        key, _, kind = spec[:3]
        default = spec[4]
        value = values.get(key, default)
        options[key] = ('1' if str(value).lower() in ('1', 'true', 'yes', 'on') else '0') if kind == 'bool' else str(value)
    api_key = ''
    selected = 0
    while True:
        names = ['Transcription path: '+backend.MODES[mode], 'Language: '+('Automatic detection' if language == 'auto' else backend.LANGUAGES[language])]
        cloud_keys = ('VOICE_CLEANUP', 'GROQ_USAGE_NOTIFY', 'VOICE_SLASH')
        visible_specs = [spec for spec in specs if mode == 'groq' or spec[0] not in cloud_keys]
        if mode == 'groq':
            names.append('Groq API key: '+('Provided key file' if key_file else 'Saved/provided' if api_key or values.get('GROQ_API_KEY') else 'Not set'))
        for spec in visible_specs:
            value = options[spec[0]]
            names.append(spec[1]+': '+(('On' if value == '1' else 'Off') if spec[2] == 'bool' else backend.option_label(value) if spec[2] == 'enum' else value))
        names.append('Install and start Frame Voice')
        selected = choose_screen(screen, 'Frame Voice installer', names, selected)
        if selected == 0:
            modes = list(backend.MODES)
            mode = modes[choose_screen(screen, 'Transcription path', list(backend.MODES.values()), modes.index(mode))]
        elif selected == 1:
            codes = ['auto', 'en']+[code for code in backend.LANGUAGES if code != 'en']
            labels = ['Automatic detection', 'English']+[backend.LANGUAGES[code] for code in codes[2:]]
            language = codes[choose_screen(screen, 'Language', labels, codes.index(language))]
        elif mode == 'groq' and selected == 2:
            api_key = entry(screen, 'Groq API key (empty keeps the saved key)', api_key, secret=True)
        elif selected == len(names)-1:
            maximum = float(options['VOICE_MAX'])
            if not 0.1 <= maximum <= 120: raise RuntimeError('Maximum recording seconds must be 0.1–120')
            if mode != 'groq':
                for key in cloud_keys: options.pop(key, None)
            return mode, language, api_key, options
        else:
            spec = visible_specs[selected-(3 if mode == 'groq' else 2)]
            key, label, kind = spec[:3]
            if kind == 'bool': options[key] = '0' if options[key] == '1' else '1'
            elif kind == 'enum':
                choices = spec[3]
                options[key] = choices[choose_screen(screen, label, [backend.option_label(value) for value in choices], choices.index(options[key]) if options[key] in choices else 0)]
            else: options[key] = entry(screen, label, options[key])


def run_tui(backend, home, requested, language, key_file):
    try:
        terminal = open('/dev/tty', 'r+')
    except OSError as error:
        raise RuntimeError('The TUI needs a terminal. Agents should use --non-interactive --backend MODE --language CODE and --groq-key-file PATH for Groq.') from error
    saved = [os.dup(fd) for fd in (0, 1)]
    try:
        for fd in (0, 1): os.dup2(terminal.fileno(), fd)
        mode, language, key, options = curses.wrapper(tui_options, backend, home, requested, language, key_file)
    finally:
        for fd, original in zip((0, 1), saved):
            os.dup2(original, fd)
            os.close(original)
        terminal.close()
    prepared, _ = backend.prepare(home, mode, key_file=key_file, api_key=key, allow_prompt=False, language=language)
    prepared.update(options)
    backend.input_permissions(os.getuid())
    backend.write_values(home/'.config/frame-voice/env', prepared)


def configure(backend, home, ui='auto', requested=None, language=None, key_file=None):
    if ui == 'auto' and requested is not None:
        backend.configure(home, requested, key_file, language=language)
        return
    environment = desktop_environment() if ui != 'tui' else None
    if ui == 'gui' or ui == 'auto' and environment:
        if not environment: raise RuntimeError('Open Desktop to use the GUI installer, or choose --tui')
        command = ['python3', str(Path(__file__).with_name('config-gui.py')), '--setup']
        if requested: command += ['--backend', requested]
        if language: command += ['--language', language]
        if key_file: command += ['--groq-key-file', str(key_file)]
        result = subprocess.run(command, env=environment)
        if result.returncode: raise RuntimeError('GUI setup canceled or failed')
        backend.input_permissions(os.getuid(), gui=True)
    else:
        run_tui(backend, home, requested, language, key_file)
