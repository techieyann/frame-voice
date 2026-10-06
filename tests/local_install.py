"""Host tests of installation/rollback and nested session discovery; no device changes."""
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch

PROJECT = Path(__file__).resolve().parent.parent
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
installer = load('local_install', PROJECT / 'tools/local-install.py')
watcher = load('tray_session', PROJECT / 'tools/tray-session.py')


class FakeServices:
    def __init__(self):
        self.state = {unit: {'active': False, 'enabled': False} for unit in (*installer.UNITS, *installer.LEGACY_UNITS)}
        self.calls = []
    def call(self, *args, required=True):
        self.calls.append(args)
        for unit in (arg for arg in args[1:] if arg in self.state):
            if args[0] == 'stop': self.state[unit]['active'] = False
            elif args[0] == 'start': self.state[unit]['active'] = True
            elif args[0] in ('enable', 'disable'):
                self.state[unit]['enabled'] = args[0] == 'enable'
                if '--now' in args: self.state[unit]['active'] = args[0] == 'enable'
        return subprocess.CompletedProcess(args, 0, b'', b'')
    def states(self, units): return {unit: self.state[unit].copy() for unit in units}
    def restore(self, states):
        for unit, state in states.items():
            self.state[unit] = state.copy()


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='frame-install-')
        self.root = Path(self.temp.name)
        self.home = self.root / 'home'
        self.home.mkdir()
        self.build = self.root / 'build'
        self.build.mkdir()
        self.binary = self.build / 'frame-voice'
        self.binary.write_text('#!/bin/sh\nif [ "$1" = --build-info ]; then echo "OpenVR compiled=true"; else exit 9; fi\n')
        self.binary.chmod(0o755)
        self.helper = self.build / 'frame-voice-paste'
        self.helper.write_bytes(b'new-helper')
        self.tray = self.build / 'frame-voice-tray'
        self.tray.write_text('#!/bin/sh\necho "StatusNotifier compiled=true"\n')
        self.tray.chmod(0o755)
        self.services = FakeServices()
        self.patches = [patch.object(installer.platform, 'system', return_value='Linux'),
                        patch.object(installer.platform, 'machine', return_value='aarch64'),
                        patch.object(installer.os, 'getuid', return_value=1000),
                        patch.object(installer.Path, 'home', return_value=self.home),
                        patch.object(installer, 'Services', return_value=self.services)]
        for item in self.patches: item.start()
    def tearDown(self):
        for item in reversed(self.patches): item.stop()
        self.temp.cleanup()
    def args(self, operation='install', **values):
        args = dict(operation=operation, binary=self.binary, tray_binary=self.tray,
                    daemon_only=False, activate=False, enable_tray=False)
        args.update(values)
        return types.SimpleNamespace(**args)
    def put(self, name, data):
        target = self.home / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        return target
    def test_fresh_install_without_credentials_and_permissions(self):
        installer.execute(self.args())  # Fake binary --check fails: install must not call it.
        self.assertEqual((self.home / '.local/bin/frame-voice-paste').read_bytes(), b'new-helper')
        self.assertEqual((self.home / installer.CONFIG).stat().st_mode & 0o777, 0o600)
        self.assertTrue((self.home / '.local/share/frame-voice/tray-icons/frame-voice-stopped.svg').exists())
        self.assertFalse(any(state['active'] for state in self.services.state.values()))
    def test_full_rollback_restores_helper_assets_unit_and_preserves_config(self):
        old = {'.local/bin/frame-voice': b'old-daemon', '.local/bin/frame-voice-paste': b'old-helper',
               '.local/share/frame-voice/actions.json': b'old-actions',
               '.config/systemd/user/frame-voice.service': b'old-unit'}
        for name, data in old.items(): self.put(name, data)
        config = self.put(installer.CONFIG, b'GROQ_API_KEY=private-do-not-backup\n')
        installer.execute(self.args())
        snapshot = installer.Transaction(self.home).current()
        for path in snapshot.iterdir():
            if path.is_file(): self.assertNotIn(b'private-do-not-backup', path.read_bytes())
        installer.execute(self.args('rollback'))
        for name, data in old.items(): self.assertEqual((self.home / name).read_bytes(), data)
        self.assertEqual(config.read_bytes(), b'GROQ_API_KEY=private-do-not-backup\n')
        self.assertFalse((self.home / '.local/bin/frame-voice-tray').exists())
    def test_mid_install_failure_restores_every_original(self):
        self.put('.local/bin/frame-voice', b'old-daemon')
        self.put('.local/bin/frame-voice-paste', b'old-helper')
        write = installer.atomic_write
        failed = False
        def fail_once(path, data, mode):
            nonlocal failed
            if path.name == 'frame-voice-paste' and not failed:
                failed = True
                raise OSError('simulated failed write')
            return write(path, data, mode)
        with patch.object(installer, 'atomic_write', side_effect=fail_once):
            with self.assertRaises(OSError): installer.execute(self.args())
        self.assertEqual((self.home / '.local/bin/frame-voice').read_bytes(), b'old-daemon')
        self.assertEqual((self.home / '.local/bin/frame-voice-paste').read_bytes(), b'old-helper')
        self.assertFalse((self.home / '.config/systemd/user/frame-voice.service').exists())
    def test_missing_helper_fails_before_any_installation(self):
        self.helper.unlink()
        with self.assertRaises(RuntimeError): installer.execute(self.args())
        self.assertFalse((self.home / '.local/bin').exists())
        self.assertEqual(self.services.calls, [])
    def test_wrong_binary_fails_before_any_installation(self):
        self.binary.write_text('#!/bin/sh\necho "OpenVR compiled=false"\n')
        with self.assertRaises(RuntimeError): installer.execute(self.args())
        self.assertFalse((self.home / '.local/bin').exists())
    def test_activation_requires_config_before_replacing_any_files(self):
        with self.assertRaises(subprocess.CalledProcessError): installer.execute(self.args(activate=True))
        self.assertFalse((self.home / '.local/bin').exists())
    def test_enable_tray_does_not_start_dictation(self):
        installer.execute(self.args(enable_tray=True))
        self.assertTrue(self.services.state[installer.UNITS[1]]['active'])
        self.assertFalse(self.services.state[installer.UNITS[0]]['active'])
    def test_running_upgrade_restores_service_state(self):
        self.binary.write_text('#!/bin/sh\necho "OpenVR compiled=true"\n')
        self.services.state[installer.UNITS[0]] = {'active': True, 'enabled': True}
        installer.execute(self.args())
        self.assertIn(('stop', installer.UNITS[0]), self.services.calls)
        self.assertEqual(self.services.state[installer.UNITS[0]], {'active': True, 'enabled': True})
    def test_reload_failure_restores_full_install_and_running_service(self):
        self.binary.write_text('#!/bin/sh\necho "OpenVR compiled=true"\n')
        self.put('.local/bin/frame-voice', b'old-daemon')
        self.put('.local/bin/frame-voice-paste', b'old-helper')
        self.services.state[installer.UNITS[0]] = {'active': True, 'enabled': True}
        call = self.services.call
        reloads = 0
        def fail_second_reload(*args, **kwargs):
            nonlocal reloads
            if args[0] == 'daemon-reload':
                reloads += 1
                if reloads == 2: raise RuntimeError('simulated reload failure')
            return call(*args, **kwargs)
        with patch.object(self.services, 'call', side_effect=fail_second_reload):
            with self.assertRaises(RuntimeError): installer.execute(self.args())
        self.assertEqual((self.home / '.local/bin/frame-voice').read_bytes(), b'old-daemon')
        self.assertEqual((self.home / '.local/bin/frame-voice-paste').read_bytes(), b'old-helper')
        self.assertFalse((self.home / '.local/bin/frame-voice-tray').exists())
        self.assertEqual(self.services.state[installer.UNITS[0]], {'active': True, 'enabled': True})
    def test_uninstall_preserves_user_data_and_dependencies(self):
        config = self.put(installer.CONFIG, b'user-config')
        dependency = self.put('.local/bin/ydotool', b'external')
        installer.execute(self.args())
        installer.execute(self.args('uninstall'))
        self.assertEqual(config.read_bytes(), b'user-config')
        self.assertEqual(dependency.read_bytes(), b'external')
        self.assertFalse((self.home / '.local/bin/frame-voice').exists())
    def test_modified_unit_blocks_rollback_before_stopping_services(self):
        installer.execute(self.args())
        self.put('.config/systemd/user/frame-voice.service', b'newer-unit')
        self.services.calls.clear()
        with self.assertRaises(RuntimeError): installer.execute(self.args('rollback'))
        self.assertEqual(self.services.calls, [])
    def test_symlink_target_is_rejected(self):
        path = self.home / '.local/bin/frame-voice'
        path.parent.mkdir(parents=True)
        path.symlink_to(self.binary)
        with self.assertRaises(RuntimeError): installer.execute(self.args())
    def test_bundled_helpers_use_private_paths_and_have_owned_units(self):
        import shutil
        project=self.root/'project'
        for folder in ('assets','tools','systemd'):
            shutil.copytree(PROJECT/folder,project/folder)
        (project/'bin').mkdir()
        for name in ('ydotool','ydotoold','ffmpeg','whisper-cli'):
            (project/'bin'/name).write_bytes(b'bundled-helper')
        legacy=self.put('.local/bin/ydotool',b'legacy-other-project')
        with patch.object(installer,'PROJECT',project): installer.execute(self.args())
        self.assertEqual(legacy.read_bytes(),b'legacy-other-project')
        self.assertEqual((self.home/'.local/share/frame-voice/bin/ydotool').read_bytes(),b'bundled-helper')
        self.assertTrue((self.home/'.config/systemd/user/frame-voice-input.service').exists())
        installer.execute(self.args('uninstall'))
        self.assertEqual(legacy.read_bytes(),b'legacy-other-project')
        self.assertFalse((self.home/'.local/share/frame-voice/bin/ydotool').exists())

    def legacy_install(self):
        import shutil
        project = self.root/'project'
        for folder in ('assets', 'tools', 'systemd'):
            shutil.copytree(PROJECT/folder, project/folder)
        (project/'bin').mkdir()
        (project/'bin/ydotoold').write_bytes(b'bundled-helper')
        for unit in installer.LEGACY_UNITS:
            self.services.state[unit] = {'active': True, 'enabled': True}
        return project

    def test_legacy_migration_and_rollback_restore_prior_service_states(self):
        project = self.legacy_install()
        with patch.object(installer, 'PROJECT', project):
            installer.execute(self.args(replace_legacy_helpers=True))
        for unit in installer.LEGACY_UNITS:
            self.assertEqual(self.services.state[unit], {'active': False, 'enabled': False})
        self.assertTrue(self.services.state['frame-voice-input.service']['active'])
        installer.execute(self.args('rollback'))
        for unit in installer.LEGACY_UNITS:
            self.assertEqual(self.services.state[unit], {'active': True, 'enabled': True})
        self.assertFalse(self.services.state['frame-voice-input.service']['active'])

    def test_shared_helpers_are_untouched_without_opt_in(self):
        project = self.legacy_install()
        with patch.object(installer, 'PROJECT', project): installer.execute(self.args())
        for unit in installer.LEGACY_UNITS:
            self.assertEqual(self.services.state[unit], {'active': True, 'enabled': True})

    def test_failed_replacement_restores_legacy_helpers(self):
        project = self.legacy_install()
        call = self.services.call
        def fail_start(*args, **kwargs):
            if args == ('start', installer.UNITS[2], installer.UNITS[3]):
                raise RuntimeError('failed bundled helper start')
            return call(*args, **kwargs)
        with patch.object(installer, 'PROJECT', project), patch.object(self.services, 'call', side_effect=fail_start):
            with self.assertRaises(RuntimeError):
                installer.execute(self.args(replace_legacy_helpers=True))
        for unit in installer.LEGACY_UNITS:
            self.assertEqual(self.services.state[unit], {'active': True, 'enabled': True})
        self.assertFalse((self.home/'.local/bin/frame-voice').exists())

    def test_uninstall_restores_retired_helpers(self):
        project = self.legacy_install()
        with patch.object(installer, 'PROJECT', project):
            installer.execute(self.args(replace_legacy_helpers=True))
        installer.execute(self.args('uninstall'))
        for unit in installer.LEGACY_UNITS:
            self.assertEqual(self.services.state[unit], {'active': True, 'enabled': True})

    def test_uninstall_keeps_current_config_and_keys_by_default(self):
        config = self.put(installer.CONFIG, b'VOICE_BACKEND=groq\n')
        key = self.put('.config/frame-voice/groq.key', b'private-key')
        installer.execute(self.args())
        installer.execute(self.args('uninstall'))
        self.assertEqual(config.read_bytes(), b'VOICE_BACKEND=groq\n')
        self.assertEqual(key.read_bytes(), b'private-key')

    def test_uninstall_removes_config_only_when_requested(self):
        self.put(installer.CONFIG, b'VOICE_BACKEND=groq\n')
        self.put('.config/frame-voice/groq.key', b'private-key')
        legacy = self.put('.config/voice-prompt/env', b'legacy-config')
        model = self.put('.local/share/frame-voice/models/model.bin', b'model')
        installer.execute(self.args())
        installer.execute(self.args('uninstall', remove_config=True))
        self.assertFalse((self.home/'.config/frame-voice').exists())
        self.assertEqual(legacy.read_bytes(), b'legacy-config')
        self.assertEqual(model.read_bytes(), b'model')

    def test_full_removal_deletes_models_app_data_and_backups(self):
        self.put(installer.CONFIG, b'VOICE_BACKEND=local\n')
        self.put('.config/frame-voice/groq.key', b'private-key')
        self.put('.local/share/frame-voice/models/ggml-base.en.bin', b'model')
        self.put('.local/share/frame-voice/custom-data.json', b'app-data')
        self.put('.local/bin/frame-voice.previous', b'backup-binary')
        self.put('.config/systemd/user/frame-voice.service.d/custom.conf', b'[Service]\n')
        legacy = self.put('.local/share/whisper/models/ggml-base.en.bin', b'legacy-model')
        installer.execute(self.args())
        with patch.object(installer, 'remove_input_rules') as cleanup:
            installer.execute(self.args('uninstall', remove_config=True, remove_models=True))
        cleanup.assert_called_once_with(1000)
        for relative in ('.config/frame-voice', '.local/share/frame-voice',
                         '.local/share/frame-voice-installer', '.local/bin/frame-voice.previous',
                         '.config/systemd/user/frame-voice.service.d'):
            self.assertFalse((self.home/relative).exists(), relative)
        self.assertEqual(legacy.read_bytes(), b'legacy-model')

    def test_full_removal_auth_cancellation_leaves_installation_intact(self):
        installer.execute(self.args())
        self.services.calls.clear()
        with patch.object(installer, 'remove_input_rules', side_effect=RuntimeError('approval canceled')):
            with self.assertRaises(RuntimeError):
                installer.execute(self.args('uninstall', remove_config=True, remove_models=True))
        self.assertTrue((self.home/'.local/bin/frame-voice').exists())
        self.assertEqual(self.services.calls, [])

    def test_model_directory_symlink_is_rejected_before_uninstall(self):
        installer.execute(self.args())
        other = self.root/'shared-models'
        other.mkdir()
        (self.home/'.local/share/frame-voice/models').symlink_to(other)
        self.services.calls.clear()
        with self.assertRaises(RuntimeError):
            installer.execute(self.args('uninstall', remove_models=True))
        self.assertEqual(self.services.calls, [])
        self.assertTrue(other.exists())

    def test_remove_config_rejects_symlink_before_stopping_services(self):
        installer.execute(self.args())
        import shutil
        shutil.rmtree(self.home/'.config/frame-voice')
        elsewhere = self.root/'other-settings'
        elsewhere.mkdir()
        (self.home/'.config/frame-voice').symlink_to(elsewhere)
        self.services.calls.clear()
        with self.assertRaises(RuntimeError):
            installer.execute(self.args('uninstall', remove_config=True))
        self.assertEqual(self.services.calls, [])
        self.assertTrue(elsewhere.exists())

    def test_upgrade_preserves_retired_helpers_for_uninstall_without_affecting_rollback(self):
        project = self.legacy_install()
        with patch.object(installer, 'PROJECT', project):
            installer.execute(self.args(replace_legacy_helpers=True))
            installer.execute(self.args())
        installer.execute(self.args('rollback'))
        for unit in installer.LEGACY_UNITS:
            self.assertEqual(self.services.state[unit], {'active': False, 'enabled': False})
        installer.execute(self.args('uninstall'))
        for unit in installer.LEGACY_UNITS:
            self.assertEqual(self.services.state[unit], {'active': True, 'enabled': True})

    def test_setup_failure_restores_previous_files_and_services(self):
        self.binary.write_text('#!/bin/sh\necho "OpenVR compiled=true"\n')
        self.put('.local/bin/frame-voice', b'old-daemon')
        self.services.state[installer.UNITS[0]] = {'active': True, 'enabled': True}
        with patch.object(installer.importlib.util, 'spec_from_file_location', side_effect=RuntimeError('setup failed')):
            with self.assertRaises(RuntimeError): installer.execute(self.args(setup=True))
        self.assertEqual((self.home/'.local/bin/frame-voice').read_bytes(), b'old-daemon')
        self.assertFalse((self.home/'.local/bin/frame-voice-tray').exists())
        self.assertEqual(self.services.state[installer.UNITS[0]], {'active': True, 'enabled': True})
        self.assertFalse((installer.Transaction(self.home).root/'current').exists())

    def test_agent_setup_saves_language_and_key_without_dialogs(self):
        project = self.legacy_install()
        for name in ('ydotool', 'ffmpeg'): (project/'bin'/name).write_bytes(b'helper')
        self.binary.write_text('#!/bin/sh\necho "OpenVR compiled=true"\n')
        key = self.root/'key'
        key.write_text('test-key')
        with patch.object(installer, 'PROJECT', project), patch.object(installer.os, 'access', return_value=True):
            installer.execute(self.args(non_interactive=True, backend='groq', language='es', groq_key_file=key))
        saved = (self.home/installer.CONFIG).read_text()
        self.assertIn('VOICE_LANG=es', saved)
        self.assertIn('GROQ_API_KEY=test-key', saved)
        self.assertTrue(self.services.state[installer.UNITS[0]]['active'])

    def test_agent_setup_never_prompts_when_key_missing(self):
        project = self.legacy_install()
        for name in ('ydotool', 'ffmpeg'): (project/'bin'/name).write_bytes(b'helper')
        with patch.object(installer, 'PROJECT', project):
            with self.assertRaises(RuntimeError):
                installer.execute(self.args(non_interactive=True, backend='groq', language='en'))
        self.assertFalse((self.home/'.local/bin/frame-voice').exists())

    def test_canceled_gui_setup_restores_config_without_secret_snapshots(self):
        from unittest.mock import Mock
        config = self.put(installer.CONFIG, b'GROQ_API_KEY=old-secret\nVOICE_BACKEND=groq\n')
        def canceled(*args):
            config.write_bytes(b'GROQ_API_KEY=new-secret\nVOICE_BACKEND=local\n')
            raise RuntimeError('administrator approval canceled')
        spec = types.SimpleNamespace(loader=Mock())
        interface = types.SimpleNamespace(configure=canceled)
        with patch.object(installer.importlib.util, 'spec_from_file_location', return_value=spec), \
             patch.object(installer.importlib.util, 'module_from_spec', side_effect=[Mock(), interface]):
            with self.assertRaises(RuntimeError): installer.execute(self.args(setup=True, setup_ui='gui'))
        self.assertEqual(config.read_bytes(), b'GROQ_API_KEY=old-secret\nVOICE_BACKEND=groq\n')
        for path in (installer.Transaction(self.home).root/'transactions').rglob('*'):
            if path.is_file():
                self.assertNotIn(b'old-secret', path.read_bytes())
                self.assertNotIn(b'new-secret', path.read_bytes())


class SessionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='frame-proc-')
        self.proc = Path(self.temp.name)
        self.uid = os.getuid()
    def tearDown(self): self.temp.cleanup()
    def process(self, pid, nested=True, bus='unix:path=/tmp/private-bus', start='42'):
        directory = self.proc / str(pid)
        directory.mkdir()
        (directory / 'comm').write_text('plasmashell\n')
        (directory / 'stat').write_text(f'{pid} (plasmashell) S ' + '0 ' * 18 + start + '\n')
        runtime = '/run/user/1000/nested_plasma' if nested else '/run/user/1000'
        (directory / 'environ').write_bytes(f'XDG_RUNTIME_DIR={runtime}\0DISPLAY=:2\0DBUS_SESSION_BUS_ADDRESS={bus}\0GROQ_API_KEY=never-read\0'.encode())
        return directory
    def test_nested_bus_is_used_and_secrets_not_copied(self):
        self.process(123)
        key, env = watcher.discover(self.uid, proc=self.proc)
        self.assertEqual(key[1], '42')
        self.assertEqual(env['DBUS_SESSION_BUS_ADDRESS'], 'unix:path=/tmp/private-bus')
        self.assertNotIn('GROQ_API_KEY', env)
    def test_outer_plasma_and_wrong_user_are_rejected(self):
        self.process(123, nested=False)
        self.assertIsNone(watcher.discover(self.uid, proc=self.proc))
        self.process(124)
        self.assertIsNone(watcher.session(124, self.uid + 1, proc=self.proc))
    def test_ambiguous_sessions_are_not_guessed(self):
        self.process(123)
        self.process(124)
        self.assertIsNone(watcher.discover(self.uid, proc=self.proc))
    def test_restarted_session_replaces_cached_bus(self):
        directory = self.process(123)
        first = watcher.discover(self.uid, proc=self.proc)
        (directory / 'environ').write_bytes(b'XDG_RUNTIME_DIR=/run/user/1000/nested_plasma\0DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/new-bus\0')
        second = watcher.discover(self.uid, cached=first[0], proc=self.proc)
        self.assertNotEqual(second[0], first[0])


if __name__ == '__main__': unittest.main()
