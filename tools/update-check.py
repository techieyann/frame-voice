#!/usr/bin/env python3
"""Manual stable-release check. Reads version information, never configuration/keys."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.error
import urllib.request

REPOSITORY = 'techieyann/frame-voice'
RELEASES = f'https://github.com/{REPOSITORY}/releases'
API = f'https://api.github.com/repos/{REPOSITORY}/releases/latest'
VERSION = re.compile(r'^v?(\d+)\.(\d+)\.(\d+)(?:-([A-Za-z0-9.]+))?(?:\+[A-Za-z0-9.]+)?$')


def version_key(value):
    match = VERSION.fullmatch(value)
    if not match:
        raise ValueError('Unsupported release version')
    return tuple(int(match[i]) for i in (1, 2, 3)) + (not bool(match[4]),)


def installed_info(home=None):
    home = Path(home or Path.home())
    info = {'version': 'Unknown', 'revision': 'unknown', 'dirty': False}
    try:
        result = subprocess.run([str(home/'.local/bin/frame-voice'), '--build-info'],
                                capture_output=True, text=True, timeout=5, check=True)
        match = re.search(r'frame-voice (\d+\.\d+\.\d+)', result.stdout)
        if match:
            info['version'] = 'v' + match[1] + '-development'
        revision = re.search(r'revision=([a-fA-F0-9]{40}|unknown)', result.stdout)
        if revision:
            info['revision'] = revision[1]
        info['dirty'] = 'dirty=true' in result.stdout
        meta = json.loads((home/'.local/share/frame-voice/release.json').read_text())
        if (isinstance(meta, dict) and not info['dirty'] and meta.get('commit') == info['revision']
                and info['revision'] != 'unknown' and isinstance(meta.get('version'), str)
                and VERSION.fullmatch(meta['version'])
                and version_key(meta['version'])[:3] == version_key(info['version'])[:3]):
            info['version'] = meta['version']
    except (OSError, ValueError, subprocess.SubprocessError):
        pass
    return info


def check(home=None, opener=urllib.request.urlopen):
    installed = installed_info(home)
    result = dict(installed=installed, status='unavailable', url=RELEASES)
    request = urllib.request.Request(API, headers={'Accept': 'application/vnd.github+json',
                                                  'User-Agent': 'frame-voice-update-check'})
    try:
        with opener(request, timeout=10) as response:
            raw = response.read(1024 * 1024 + 1)
        if len(raw) > 1024 * 1024:
            raise ValueError('Release response too large')
        release = json.loads(raw)
        if not isinstance(release, dict) or not isinstance(release.get('tag_name'), str):
            raise ValueError('Invalid release response')
        latest = release['tag_name']
        key = version_key(latest)
        if release.get('draft') or release.get('prerelease') or not key[-1]:
            raise ValueError('Expected stable release')
        # Construct the link ourselves; no arbitrary response URL is opened.
        result.update(latest=latest, url=RELEASES+'/tag/'+latest)
        try:
            available = key > version_key(installed['version'])
        except ValueError:
            available = True
        result.update(status='available' if available else 'current')
    except urllib.error.HTTPError as error:
        result['message'] = 'No public stable release was found.' if error.code == 404 else 'The release check failed. Try again later.'
    except (OSError, ValueError, KeyError):
        result['message'] = 'Could not check releases. Check your connection and try again.'
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--json', action='store_true')
    parser.add_argument('--home', type=Path)
    args = parser.parse_args()
    result = check(args.home)
    if args.json:
        print(json.dumps(result))
    else:
        print('Installed:', result['installed']['version'])
        if 'latest' in result:
            print('Latest:', result['latest'])
        print('Update available.' if result['status'] == 'available' else
              'Up to date.' if result['status'] == 'current' else result['message'])
        print(result['url'])
    return 0 if result['status'] != 'unavailable' else 1


if __name__ == '__main__':
    raise SystemExit(main())
