"""GTK integration checks. Run on Linux with xvfb-run; no real service changes."""
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch

try:
    import gi
    gi.require_version('Adw', '1')
    from gi.repository import Adw as RequiredAdw
except (ImportError, ValueError):
    raise unittest.SkipTest('GUI tests require GTK4/libadwaita on Linux')
if not (os.environ.get('DISPLAY') or os.environ.get('WAYLAND_DISPLAY')):
    raise unittest.SkipTest('GUI tests require a display; run under xvfb-run')
if RequiredAdw.get_major_version() == 1 and RequiredAdw.get_minor_version() < 4:
    raise unittest.SkipTest('GUI tests require libadwaita 1.4 or newer')

spec = importlib.util.spec_from_file_location(
    'config_gui', Path(__file__).resolve().parent.parent/'tools/config-gui.py')
gui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gui)
Gtk, GLib, Adw = gui.Gtk, gui.GLib, gui.Adw


class ConfigTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.config = Path(self.temp.name)/'config/env'
        self.config.parent.mkdir()
        self.config.write_text('VOICE_BEEP=0\nVRBTN_DICTATE_LEFT=0\nCUSTOM_SETTING=kept\n')
        self.config_patch = patch.object(gui, 'CONFIG', self.config)
        self.config_patch.start()
        self.env_patch = patch.object(gui.backend, 'read_values', return_value={})
        self.env_patch.start()
        self.prepare_patch = patch.object(gui.backend, 'prepare', return_value=({'VOICE_BACKEND': 'groq'}, 'groq'))
        self.prepare = self.prepare_patch.start()
        self.app = gui.ConfigApp('general')
        self.app.set_application_id('dev.framevoice.test.case' + str(time.time_ns()))
        self.app.register(None)
        self.app.activate()
        self.run_patch = patch.object(gui.subprocess, 'run', return_value=
                                     subprocess.CompletedProcess([], 0, '', ''))
        self.run = self.run_patch.start()

    def tearDown(self):
        for window in list(Gtk.Window.get_toplevels()):
            window.destroy()
        self.run_patch.stop()
        self.prepare_patch.stop()
        self.env_patch.stop()
        self.config_patch.stop()
        self.temp.cleanup()

    def confirmation(self):
        self.app.confirm_uninstall()
        return next(window for window in Gtk.Window.get_toplevels()
                    if isinstance(window, Adw.MessageDialog))

    def wait_for_uninstall(self):
        deadline = time.monotonic() + 3
        while self.app.busy and time.monotonic() < deadline:
            GLib.MainContext.default().iteration(False)
            time.sleep(0.01)
        self.assertFalse(self.app.busy)

    def test_tabs_current_values_and_save_both_sections(self):
        pages = list(self.app.tabs.get_pages())
        self.assertEqual([page.get_title() for page in pages],
                         ['General', 'Transcription', 'Controllers'])
        self.assertEqual(Adw.StyleManager.get_default().get_color_scheme(), Adw.ColorScheme.FORCE_DARK)
        fields = {widget.key: widget for widget in self.app.fields}
        self.assertFalse(fields['VOICE_BEEP'].get_active())
        self.assertFalse(fields['VRBTN_DICTATE_LEFT'].get_active())
        fields['VOICE_BEEP'].set_active(True)
        fields['VRBTN_DICTATE_LEFT'].set_active(True)
        self.app.on_save()
        self.wait_for_uninstall()
        saved = self.config.read_text()
        self.assertIn('VOICE_BEEP=1', saved)
        self.assertIn('VRBTN_DICTATE_LEFT=1', saved)
        self.assertIn('CUSTOM_SETTING=kept', saved)
        self.assertEqual(self.config.stat().st_mode & 0o777, 0o600)

    def test_update_check_is_read_only_and_update_waits_for_saved_settings(self):
        result = {'status': 'available', 'installed': {'version': 'v0.1.0-development'},
                  'latest': 'v0.1.0', 'url': 'https://github.com/techieyann/frame-voice/releases/tag/v0.1.0'}
        self.app.show_updates(result)
        dialog = next(window for window in Gtk.Window.get_toplevels()
                      if isinstance(window, Adw.MessageDialog))
        self.assertTrue(dialog.get_response_enabled('update'))
        self.assertFalse(self.app.save_button.get_sensitive())
        dialog.destroy()
        fields = {widget.key: widget for widget in self.app.fields}
        fields['VOICE_BEEP'].set_active(True)
        self.app.show_updates(result)
        dialog = next(window for window in Gtk.Window.get_toplevels()
                      if isinstance(window, Adw.MessageDialog))
        self.assertFalse(dialog.get_response_enabled('update'))
        self.assertIn('Save settings', dialog.get_body())

    def test_save_disabled_until_changed_and_again_after_save(self):
        fields = {widget.key: widget for widget in self.app.fields}
        self.assertFalse(self.app.save_button.get_sensitive())
        self.app.tabs.set_visible_child_name('controllers')
        self.assertFalse(self.app.save_button.get_sensitive())
        fields['VOICE_BEEP'].set_active(True)
        self.assertTrue(self.app.save_button.get_sensitive())
        fields['VOICE_BEEP'].set_active(False)
        self.assertFalse(self.app.save_button.get_sensitive())
        fields['VOICE_BEEP'].set_active(True)
        self.app.on_save()
        self.wait_for_uninstall()
        self.assertFalse(self.app.save_button.get_sensitive())

    def test_human_button_labels_keep_underlying_binding_codes(self):
        fields = {widget.key: widget for widget in self.app.fields}
        right = fields['VRBTN_SUBMIT_RIGHT']
        left = fields['VRBTN_SUBMIT_LEFT']
        self.assertEqual(right.get_selected_item().get_string(), 'A')
        self.assertEqual(left.get_selected_item().get_string(), 'D-pad Right')
        self.assertEqual(self.app.collect_settings()['VRBTN_SUBMIT_RIGHT'], 'a')
        self.assertEqual(self.app.collect_settings()['VRBTN_SUBMIT_LEFT'], 'dpad_right')
        self.assertNotIn('VOICE_NOTIFY', fields)
        self.assertIn('GROQ_USAGE_NOTIFY', fields)

    def test_prompt_editor_apply_and_reset_use_shared_save(self):
        path = self.config.parent/'groq-prompt.txt'
        self.app.open_prompt()
        dialog = next(w for w in Gtk.Window.get_toplevels() if isinstance(w, Adw.MessageDialog))
        self.app.prompt_editor.get_buffer().set_text('My custom terminology.')
        dialog.response('apply')
        self.assertTrue(self.app.save_button.get_sensitive())
        self.assertFalse(path.exists())
        self.app.on_save()
        self.wait_for_uninstall()
        self.assertEqual(path.read_text(), 'My custom terminology.')
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.app.open_prompt()
        dialog = next(w for w in Gtk.Window.get_toplevels() if isinstance(w, Adw.MessageDialog))
        content = dialog.get_extra_child()
        reset = content.get_last_child()
        reset.emit('clicked')
        dialog.response('apply')
        self.assertEqual(self.app.prompt_text, self.app.default_prompt())
        self.assertEqual(path.read_text(), 'My custom terminology.')
        self.app.on_save()
        self.wait_for_uninstall()
        self.assertEqual(path.read_text(), self.app.default_prompt())

    def test_cancel_does_not_uninstall(self):
        dialog = self.confirmation()
        self.assertTrue(self.app.keep_config.get_active())
        dialog.response('cancel')
        self.run.assert_not_called()

    def test_confirm_keeps_config_by_default(self):
        self.confirmation().response('uninstall')
        self.wait_for_uninstall()
        command = self.run.call_args.args[0]
        self.assertEqual(command[-1], 'uninstall')
        self.assertNotIn('--remove-config', command)
        self.assertFalse(self.app.uninstall_button.get_sensitive())

    def test_confirm_remove_config(self):
        dialog = self.confirmation()
        self.app.keep_config.set_active(False)
        dialog.response('uninstall')
        self.wait_for_uninstall()
        self.assertEqual(self.run.call_args.args[0][-2:], ['uninstall', '--remove-config'])

    def test_confirm_remove_models_and_config(self):
        dialog = self.confirmation()
        self.app.keep_config.set_active(False)
        self.app.keep_models.set_active(False)
        dialog.response('uninstall')
        self.wait_for_uninstall()
        command = self.run.call_args.args[0]
        self.assertIn('--remove-models', command)
        self.assertIn('--remove-config', command)

    def test_transcription_selection_hides_groq_key_for_local(self):
        self.assertTrue(self.app.groq_group.get_visible())
        self.app.backend_choice.set_selected(1)
        self.assertFalse(self.app.groq_group.get_visible())
        self.assertTrue(self.app.local_group.get_visible())
        self.app.on_save()
        self.wait_for_uninstall()
        self.assertEqual(self.prepare.call_args.args[1], 'local-fast')
        self.assertFalse(self.prepare.call_args.kwargs['allow_prompt'])

    def test_download_failure_keeps_config_and_service_running(self):
        before = self.config.read_bytes()
        self.app.backend_choice.set_selected(2)
        self.prepare.side_effect = RuntimeError('Download failed')
        self.app.on_save()
        self.wait_for_uninstall()
        self.assertEqual(self.config.read_bytes(), before)
        self.run.assert_not_called()
        self.assertEqual(self.app.status.get_text(), 'Download failed')

    def test_model_delete_requires_confirmation_and_keeps_settings(self):
        before = self.config.read_bytes()
        with patch.object(gui.backend, 'delete_model') as delete:
            self.app.confirm_delete_model('local-fast', 'Local · Fast', 142)
            dialog = next(w for w in Gtk.Window.get_toplevels() if isinstance(w, Adw.MessageDialog))
            dialog.response('cancel')
            delete.assert_not_called()
            self.app.confirm_delete_model('local-fast', 'Local · Fast', 142)
            dialog = next(w for w in Gtk.Window.get_toplevels() if isinstance(w, Adw.MessageDialog))
            dialog.response('delete')
            delete.assert_called_once()
        self.assertEqual(self.config.read_bytes(), before)
        self.run.assert_not_called()

    def test_active_model_delete_disabled_until_save(self):
        model = Path(self.temp.name)/'ggml-base.en.bin'
        model.write_bytes(b'model')
        with patch.object(gui.backend, 'downloaded_models', return_value=[('local-fast', model)]), \
             patch.object(gui.backend, 'model_in_use', return_value=True) as in_use:
            self.app.refresh_models()
            self.assertFalse(self.app.model_buttons['local-fast'].get_sensitive())
            self.app.backend_choice.set_selected(0)
            self.assertFalse(self.app.model_buttons['local-fast'].get_sensitive())
            in_use.return_value = False
            self.app.save_finished('')
            self.assertTrue(self.app.model_buttons['local-fast'].get_sensitive())

    def test_language_choice_is_passed_to_backend(self):
        self.app.language_choice.set_selected(self.app.language_codes.index('es'))
        self.app.on_save()
        self.wait_for_uninstall()
        self.assertEqual(self.prepare.call_args.kwargs['language'], 'es')
        self.app.language_choice.set_selected(0)
        self.app.on_save()
        self.wait_for_uninstall()
        self.assertEqual(self.prepare.call_args.kwargs['language'], 'auto')

    def test_gui_installer_hides_uninstall_and_defers_service_activation(self):
        self.app.window.destroy()
        self.app = gui.ConfigApp('transcription', setup=True, requested='local-fast', language='fr')
        self.app.set_application_id('dev.framevoice.test.setup'+str(time.time_ns()))
        self.app.register(None)
        self.app.activate()
        self.assertEqual(len(list(self.app.tabs.get_pages())), 3)
        self.assertFalse(self.app.saved)
        self.app.on_save()
        self.wait_for_uninstall()
        self.assertTrue(self.app.saved)
        self.assertEqual(self.prepare.call_args.kwargs['language'], 'fr')
        self.assertEqual(self.prepare.call_args.args[1], 'local-fast')
        self.assertEqual(len(self.run.call_args_list), 1)  # --check only; installer activates.

    def test_failure_is_visible_and_can_retry(self):
        self.run.return_value = subprocess.CompletedProcess([], 1, '', 'Could not stop service')
        self.confirmation().response('uninstall')
        self.wait_for_uninstall()
        self.assertEqual(self.app.status.get_text(), 'Could not stop service')
        self.assertTrue(self.app.uninstall_button.get_sensitive())


if __name__ == '__main__':
    unittest.main()
