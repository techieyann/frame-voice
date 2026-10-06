#!/usr/bin/env python3
"""Run the tray only in the observed nested Plasma session, not the outer bus."""
import argparse
import fcntl
import os
from pathlib import Path
import signal
import subprocess
import time

FIELDS = {'DBUS_SESSION_BUS_ADDRESS', 'DISPLAY', 'WAYLAND_DISPLAY', 'XAUTHORITY',
          'XDG_RUNTIME_DIR', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_DATA_DIRS',
          'XDG_CURRENT_DESKTOP', 'XDG_SESSION_TYPE', 'DESKTOP_SESSION',
          'KDE_FULL_SESSION', 'KDE_SESSION_VERSION'}


def session(pid, uid, proc=Path('/proc')):
    directory = proc / str(pid)
    try:
        if directory.stat().st_uid != uid or (directory / 'comm').read_text().strip() != 'plasmashell':
            return None
        env = {}
        for field in (directory / 'environ').read_bytes().split(b'\0'):
            key, separator, value = field.partition(b'=')
            name = key.decode(errors='replace')
            if separator and name in FIELDS:
                env[name] = value.decode(errors='surrogateescape')
        nested = any('nested_plasma' in env.get(name, '') for name in ('XDG_RUNTIME_DIR', 'XAUTHORITY'))
        if not nested or not env.get('DBUS_SESSION_BUS_ADDRESS'):
            return None
        start = (directory / 'stat').read_text().rsplit(')', 1)[1].split()[19]
        key = (str(pid), start, env['DBUS_SESSION_BUS_ADDRESS'])
        return key, env
    except (OSError, IndexError):
        return None


def discover(uid, cached=None, proc=Path('/proc')):
    if cached:
        found = session(cached[0], uid, proc)
        if found and found[0] == cached:
            return found
    matches = []
    for directory in proc.iterdir():
        if directory.name.isdigit():
            found = session(directory.name, uid, proc)
            if found:
                matches.append(found)
    # Never choose an arbitrary session if two nested Desktops exist.
    return matches[0] if len(matches) == 1 else None


def stop(child):
    if child is not None:
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill()
        child.wait()


def run(binary):
    uid = os.getuid()
    runtime = Path('/run/user') / str(uid)
    lock = open(runtime / 'frame-voice-tray-session.lock', 'a')
    os.chmod(lock.name, 0o600)
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        return
    running = True
    def exit_requested(_signal, _frame):
        nonlocal running
        running = False
    signal.signal(signal.SIGTERM, exit_requested)
    signal.signal(signal.SIGINT, exit_requested)
    child, current, suppressed = None, None, None
    retry_at = 0
    print('Watching for the nested SteamOS Desktop session', flush=True)
    try:
        while running:
            found = discover(uid, current)
            key = found[0] if found else None
            if key != current:
                stop(child)
                child, current, suppressed = None, key, None
                retry_at = 0
                print(f'Nested Desktop PID {key[0]} discovered' if key else 'Nested Desktop closed; tray stopped', flush=True)
            if child is not None and child.poll() is not None:
                code = child.wait()
                child = None
                if code == 0:
                    suppressed = current  # Hide lasts until the next Desktop session.
                else:
                    retry_at = time.monotonic() + 10
                    print(f'Tray exited ({code}); retrying in 10 seconds', flush=True)
            if found and child is None and suppressed != current and time.monotonic() >= retry_at:
                environment = {key: value for key, value in os.environ.items() if key not in FIELDS}
                environment.update(found[1])
                try:
                    child = subprocess.Popen([str(binary)], env=environment)
                except OSError as error:
                    print(f'Tray launch failed: {error}', flush=True)
                    retry_at = time.monotonic() + 10
            time.sleep(2)
    finally:
        stop(child)
        lock.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--probe', action='store_true', help='Report session discovery without starting anything')
    parser.add_argument('--binary', type=Path, default=Path.home() / '.local/bin/frame-voice-tray')
    args = parser.parse_args()
    if args.probe:
        found = discover(os.getuid())
        if found:
            print(f'Nested Plasma PID {found[0][0]} found; display={found[1].get("DISPLAY", "")}; private session bus present')
        else:
            print('No single eligible nested Plasma session found. Open Desktop and retry.')
    else:
        run(args.binary)
