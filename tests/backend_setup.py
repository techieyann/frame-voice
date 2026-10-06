"""First-run choices, model integrity, cache use and credential preservation."""
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

PROJECT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('backend_setup', PROJECT/'tools/setup-backend.py')
backend=importlib.util.module_from_spec(spec)
spec.loader.exec_module(backend)


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory(prefix='frame-backend-')
        self.home=Path(self.temporary.name)
        self.helpers=self.home/'.local/share/frame-voice/bin'
        self.helpers.mkdir(parents=True)
        for name in ('ffmpeg','ydotool','ydotoold','whisper-cli'): (self.helpers/name).write_bytes(b'fixture')
        self.config=self.home/'.config/frame-voice/env'
    def tearDown(self): self.temporary.cleanup()
    def test_cloud_requires_key_and_writes_private_config(self):
        key=self.home/'key'
        key.write_text('secret-test-value')
        backend.configure(self.home, 'groq', key, allow_permissions=False)
        values=backend.read_values(self.home)
        self.assertEqual(values['GROQ_API_KEY'], 'secret-test-value')
        self.assertEqual(values['VOICE_BACKEND'], 'groq')
        self.assertEqual(self.config.stat().st_mode & 0o777, 0o600)
        self.assertEqual(values['YDOTOOL_BIN'], str(self.helpers/'ydotool'))
        key.write_text('')
        before=self.config.read_bytes()
        with self.assertRaises(RuntimeError): backend.configure(self.home, 'groq', key, allow_permissions=False)
        self.assertEqual(self.config.read_bytes(),before)
    def test_existing_cloud_configuration_does_not_prompt_again(self):
        backend.write_values(self.config, {'GROQ_API_KEY':'saved-secret','VOICE_BACKEND':'groq','VOICE_MAX':'15'})
        with patch.object(backend,'choose',side_effect=AssertionError('unexpected prompt')):
            backend.configure(self.home, allow_permissions=False)
        self.assertEqual(backend.read_values(self.home)['VOICE_MAX'],'15')
    def test_explicit_helper_overrides_survive_setup(self):
        backend.write_values(self.config, {'GROQ_API_KEY':'saved-secret','VOICE_BACKEND':'groq',
                                          'YDOTOOL_BIN':'/custom/input','YDOTOOL_SOCKET':'/custom/socket'})
        backend.configure(self.home, allow_permissions=False)
        values=backend.read_values(self.home)
        self.assertEqual(values['YDOTOOL_BIN'],'/custom/input')
        self.assertEqual(values['YDOTOOL_SOCKET'],'/custom/socket')
    def test_local_choice_downloads_model_without_a_key(self):
        model=self.home/'model'
        model.write_bytes(b'weights')
        with patch.object(backend,'download_model',return_value=model) as download:
            backend.configure(self.home,'local-balanced',allow_permissions=False)
        download.assert_called_once_with(self.home,'local-balanced')
        values=backend.read_values(self.home)
        self.assertEqual(values['VOICE_BACKEND'],'local')
        self.assertEqual(values['VOICE_CLEANUP'],'0')
        self.assertEqual(values['VOICE_MODEL'],str(model))
        self.assertNotIn('GROQ_API_KEY',values)
    def test_missing_helper_fails_before_writing_config(self):
        (self.helpers/'ydotool').unlink()
        with self.assertRaises(RuntimeError): backend.configure(self.home,'groq',allow_permissions=False)
        self.assertFalse(self.config.exists())
    def test_verified_model_is_cached_for_offline_reuse(self):
        weights=b'valid model weights'
        expected=hashlib.sha1(weights).hexdigest()
        with patch.dict(backend.MODELS,{'local-fast':('base.en',expected)}):
            path=backend.download_model(self.home,'local-fast',opener=lambda *args,**kw: io.BytesIO(weights))
            self.assertEqual(path.read_bytes(),weights)
            same=backend.download_model(self.home,'local-fast',opener=lambda *args,**kw: self.fail('cache used network'))
            self.assertEqual(path,same)
    def test_failed_download_preserves_existing_model(self):
        directory=self.home/'.local/share/frame-voice/models'
        directory.mkdir()
        path=directory/'ggml-base.en.bin'
        path.write_bytes(b'existing weights')
        with self.assertRaises(RuntimeError):
            backend.download_model(self.home,'local-fast',opener=lambda *args,**kw: io.BytesIO(b'bad response'))
        self.assertEqual(path.read_bytes(),b'existing weights')
        self.assertEqual(list(directory.iterdir()),[path])
    def test_invalid_mode_does_not_modify_config(self):
        with self.assertRaises(RuntimeError): backend.configure(self.home,'invalid',allow_permissions=False)
        self.assertFalse(self.config.exists())
    def test_updates_preserve_settings_and_remove_duplicate_updated_keys(self):
        self.config.parent.mkdir(parents=True)
        self.config.write_text('# note\nVOICE_BACKEND=groq\nVOICE_MAX=12\nVOICE_BACKEND=groq\n')
        backend.write_values(self.config,{'VOICE_BACKEND':'local'})
        self.assertEqual(self.config.read_text().count('VOICE_BACKEND='),1)
        self.assertIn('VOICE_MAX=12',self.config.read_text())
    def test_input_setup_only_runs_when_needed_and_never_starts_root_helpers(self):
        with patch.object(backend.os,'access',return_value=True), patch.object(backend.subprocess,'run') as run:
            backend.input_permissions(1000)
            run.assert_not_called()
        with patch.object(backend.os,'access',side_effect=[False,True]), patch.object(backend.subprocess,'run') as run:
            backend.input_permissions(1000)
        commands=[call.args[0] for call in run.call_args_list]
        self.assertEqual(commands[0][:2],['sudo','install'])
        self.assertTrue(all('ydotoold' not in command for command in commands))
        self.assertIn(['sudo','udevadm','settle'],commands)


if __name__=='__main__': unittest.main()
