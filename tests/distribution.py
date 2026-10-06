"""Test release contents and authenticated channel installs without network/device access."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

PROJECT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('release_pack', PROJECT / 'tools/package-release.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def test_stable_and_experimental_tags_are_distinct(self):
        self.assertEqual(release.classify('v0.1.0'), ('0.1.0', 'stable'))
        self.assertEqual(release.classify('v0.1.0-experimental.1'), ('0.1.0', 'experimental'))
        for tag in ('main', 'v1.0', 'v01.0.0', 'v1.0.0-beta.1', 'v1.0.0-experimental.0', '../v1.0.0'):
            with self.assertRaises(ValueError): release.classify(tag)
    def test_versions_match_manifests(self):
        self.assertEqual(release.version(PROJECT / 'Cargo.toml'), release.version(PROJECT / 'tray/Cargo.toml'))
    def test_runtime_allowlist_excludes_handoffs_tests_and_build_state(self):
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            for name in ('frame-voice', 'frame-voice-paste', 'frame-voice-tray'):
                (build / name).write_bytes(b'fixture-binary')
            license_file = build / 'OpenVR.txt'
            license_file.write_text('fixture-license')
            contents = release.files(PROJECT, build, build/'frame-voice-tray', license_file)
            self.assertIn('bin/frame-voice-paste', contents)
            self.assertIn('tools/config-gui.py', contents)
            self.assertIn('licenses/OpenVR.txt', contents)
            self.assertTrue(all(not name.startswith(('src/', 'tests/', 'docs/', 'target/')) for name in contents))
            self.assertFalse(any('HANDOFF' in name.upper() for name in contents))
            (build/'frame-voice-paste').unlink()
            with self.assertRaises(ValueError): release.files(PROJECT, build, build/'frame-voice-tray', license_file)
    def test_archive_metadata_permissions_and_checksums_are_deterministic(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            contents = {'bin/frame-voice': (b'program', 0o755), 'assets/actions.json': (b'{}', 0o644)}
            archive = release.pack(output, 'v0.1.0-experimental.1', 'a'*40, contents)
            first = archive.read_bytes()
            release.pack(output, 'v0.1.0-experimental.1', 'a'*40, contents)
            self.assertEqual(archive.read_bytes(), first)
            with tarfile.open(archive) as bundle:
                metadata = json.load(bundle.extractfile('frame-voice/release.json'))
                self.assertEqual(metadata['channel'], 'experimental')
                self.assertEqual(metadata['target'], 'aarch64-unknown-linux-gnu')
                self.assertEqual(bundle.getmember('frame-voice/bin/frame-voice').mode, 0o755)
                for line in bundle.extractfile('frame-voice/FILES-SHA256SUMS').read().decode().splitlines():
                    expected, name = line.split('  ', 1)
                    self.assertEqual(hashlib.sha256(bundle.extractfile('frame-voice/'+name).read()).hexdigest(), expected)


class DownloadTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='frame-download-')
        self.root = Path(self.temporary.name)
        self.commands = self.root/'commands'
        self.commands.mkdir()
        self.marker = self.root/'installed'
        self.calls = self.root/'gh-calls'
        self.write('uname', '#!/bin/sh\nif [ "$1" = -s ]; then echo Linux; else echo aarch64; fi\n')
        # GitHub fixture emits an API selection and copies release assets; no network.
        self.write('gh', '''#!/usr/bin/env python3
import os, shutil, sys
from pathlib import Path
with open(os.environ['GH_CALLS'], 'a') as stream: stream.write(' '.join(sys.argv[1:])+'\\n')
if sys.argv[1] == 'api': print(os.environ['SELECTED_TAG'])
else:
    destination = Path(sys.argv[sys.argv.index('--dir')+1])
    for path in Path(os.environ['FIXTURE_ASSETS']).iterdir(): shutil.copy2(path, destination/path.name)
''')
        # sha256sum is standard on Frame but not macOS.
        self.write('sha256sum', '''#!/usr/bin/env python3
import hashlib, sys
from pathlib import Path
if sys.argv[1] == '-c':
    for line in Path(sys.argv[2]).read_text().splitlines():
        expected, name = line.split('  ', 1)
        if hashlib.sha256(Path(name).read_bytes()).hexdigest() != expected: raise SystemExit(1)
else: print(hashlib.sha256(Path(sys.argv[1]).read_bytes()).hexdigest()+'  '+sys.argv[1])
''')
        self.env = dict(os.environ, PATH=str(self.commands)+os.pathsep+os.environ['PATH'],
                        INSTALL_MARKER=str(self.marker), GH_CALLS=str(self.calls))
    def tearDown(self): self.temporary.cleanup()
    def write(self, name, source):
        path = self.commands/name
        path.write_text(source)
        path.chmod(0o755)
    def assets(self, tag):
        output = self.root/'assets'
        output.mkdir()
        content = {'install.sh': (b'#!/bin/sh\nprintf "%s\\n" "$@" > "$INSTALL_MARKER"\n', 0o755)}
        archive = release.pack(output, tag, 'b'*40, content)
        self.env.update(SELECTED_TAG=tag, FIXTURE_ASSETS=str(output))
        return archive
    def invoke(self, channel):
        return subprocess.run(['sh', str(PROJECT/'get.sh'), '--channel', channel, '--enable-tray'],
                              env=self.env, capture_output=True, text=True)
    def test_stable_selection_installs_verified_bundle_and_forwards_flags(self):
        self.assets('v0.1.0')
        result = self.invoke('stable')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('releases/latest', self.calls.read_text())
        self.assertIn('--enable-tray', self.marker.read_text())
        self.assertIn('./bin/frame-voice-tray', self.marker.read_text())
    def test_experimental_selection_uses_prerelease_query(self):
        self.assets('v0.1.0-experimental.1')
        result = self.invoke('experimental')
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls.read_text()
        self.assertIn('--paginate --slurp', calls)
        self.assertIn('.prerelease == true', calls)
        self.assertNotIn('releases/latest', calls)
    def test_piped_script_downloads_and_installs_without_source_checkout(self):
        self.assets('v0.1.0')
        result = subprocess.run(['sh', '-s', '--', '--channel', 'stable', '--enable-tray'],
                                input=(PROJECT/'get.sh').read_text(), env=self.env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(self.marker.exists())
    def test_checksum_failure_never_invokes_installer(self):
        archive = self.assets('v0.1.0')
        archive.write_bytes(archive.read_bytes()+b'corrupted')
        result = self.invoke('stable')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('checksum mismatch', result.stderr)
        self.assertFalse(self.marker.exists())
    def test_wrong_channel_metadata_never_invokes_installer(self):
        self.assets('v0.1.0-experimental.1')
        result = self.invoke('stable')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('metadata does not match', result.stderr)
        self.assertFalse(self.marker.exists())


if __name__ == '__main__': unittest.main()
