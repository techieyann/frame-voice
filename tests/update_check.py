"""Version comparison/network failures without network or device access."""
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
import urllib.error
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('updates', Path(__file__).resolve().parent.parent/'tools/update-check.py')
updates = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updates)


class UpdateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.home = Path(self.temp.name)
        self.meta = self.home/'.local/share/frame-voice/release.json'
        self.meta.parent.mkdir(parents=True)
        self.revision = 'a'*40
        self.meta.write_text(json.dumps({'version': 'v0.1.0', 'commit': self.revision}))
        self.binary = patch.object(updates.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0,
            f'frame-voice 0.1.0; revision={self.revision}; dirty=false; OpenVR compiled=true', ''))
        self.run = self.binary.start()
    def tearDown(self):
        self.binary.stop()
        self.temp.cleanup()
    def fetch(self, tag='v0.1.0', **extra):
        return lambda *args, **kwargs: io.BytesIO(json.dumps(dict(tag_name=tag, prerelease=False, draft=False, **extra)).encode())
    def test_current_release_and_newer_version(self):
        self.assertEqual(updates.check(self.home, self.fetch())['status'], 'current')
        result = updates.check(self.home, self.fetch('v0.2.0', html_url='https://untrusted.invalid'))
        self.assertEqual(result['status'], 'available')
        self.assertEqual(result['url'], updates.RELEASES+'/tag/v0.2.0')
    def test_development_and_experimental_can_update_to_same_stable_base(self):
        self.run.return_value.stdout = f'frame-voice 0.1.0; revision={self.revision}; dirty=true; OpenVR compiled=true'
        self.assertEqual(updates.check(self.home, self.fetch())['status'], 'available')
        self.run.return_value.stdout = self.run.return_value.stdout.replace('dirty=true', 'dirty=false')
        self.meta.write_text(json.dumps({'version': 'v0.1.0-experimental.2', 'commit': self.revision}))
        self.assertEqual(updates.check(self.home, self.fetch())['status'], 'available')
    def test_mismatched_metadata_and_malformed_json_are_not_trusted(self):
        self.meta.write_text(json.dumps({'version': 'v9.0.0', 'commit': self.revision}))
        self.assertEqual(updates.installed_info(self.home)['version'], 'v0.1.0-development')
        self.meta.write_text('[]')
        self.assertEqual(updates.installed_info(self.home)['version'], 'v0.1.0-development')
        self.assertEqual(updates.check(self.home, lambda *a, **k: io.BytesIO(b'[]'))['status'], 'unavailable')
    def test_network_unavailable_and_private_or_unpublished_release(self):
        def offline(*args, **kwargs): raise OSError('offline')
        self.assertEqual(updates.check(self.home, offline)['status'], 'unavailable')
        def not_found(*args, **kwargs): raise urllib.error.HTTPError(updates.API, 404, 'not found', {}, None)
        self.assertIn('No public stable', updates.check(self.home, not_found)['message'])
    def test_does_not_downgrade_or_select_prereleases(self):
        self.run.return_value.stdout = f'frame-voice 0.2.0; revision={self.revision}; dirty=false; OpenVR compiled=true'
        self.meta.write_text(json.dumps({'version': 'v0.2.0', 'commit': self.revision}))
        self.assertEqual(updates.check(self.home, self.fetch())['status'], 'current')
        self.assertEqual(updates.check(self.home, self.fetch('v0.3.0-experimental.1'))['status'], 'unavailable')

if __name__ == '__main__': unittest.main()
