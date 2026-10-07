#!/usr/bin/env python3
"""Dark libadwaita settings, backend selection, and confirmed removal."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import threading

import gi

gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Adw, Gtk, GLib

spec = importlib.util.spec_from_file_location('backend_setup', Path(__file__).with_name('setup-backend.py'))
backend = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backend)
update_spec = importlib.util.spec_from_file_location('frame_update_check', Path(__file__).with_name('update-check.py'))
updates = importlib.util.module_from_spec(update_spec)
update_spec.loader.exec_module(updates)
CONFIG = Path.home()/'.config/frame-voice/env'
SPECS = backend.SETTINGS


def read_env():
    values = backend.read_values(Path.home())
    if CONFIG.is_file():
        for line in CONFIG.read_text().splitlines():
            if line.strip() and not line.lstrip().startswith('#') and '=' in line:
                key, value = line.split('=', 1)
                values[key.strip()] = value.strip()
    return values


def truthy(value, default):
    return default if value is None else value.lower() in ('1', 'true', 'yes', 'on')


def save(updates):
    backend.write_values(CONFIG, updates)


def service_environment():
    uid = os.getuid()
    return dict(os.environ, DBUS_SESSION_BUS_ADDRESS=f'unix:path=/run/user/{uid}/bus',
                XDG_RUNTIME_DIR=f'/run/user/{uid}')


class ConfigApp(Adw.Application):
    def __init__(self, mode='general', setup=False, requested=None, language=None, key_file=None):
        super().__init__(application_id='dev.framevoice.setup' if setup else 'dev.framevoice.config')
        self.mode = mode
        self.setup = setup
        self.requested = requested
        self.language = language
        self.key_file = key_file
        self.saved = False
        self.fields = []
        self.busy = False

    def do_activate(self):
        if hasattr(self, 'window'):
            self.window.present()
            return
        Adw.StyleManager.get_default().set_color_scheme(Adw.ColorScheme.FORCE_DARK)
        Gtk.Window.set_default_icon_name('frame-voice')
        self.window = Adw.ApplicationWindow(application=self, title='Install Frame Voice' if self.setup else 'Frame Voice Configuration')
        self.window.set_icon_name('frame-voice')
        Gtk.IconTheme.get_for_display(self.window.get_display()).add_search_path(
            str(Path.home()/'.local/share/icons/hicolor/scalable/apps'))
        self.window.set_default_size(780, 660)
        self.window.set_size_request(720, 560)
        self.window.connect('close-request', lambda *_: self.busy)
        self.tabs = Adw.ViewStack()
        self.current = read_env()
        self.prompt_text = self.read_prompt()
        for name, title, icon in (
            ('general', 'General', 'preferences-system-symbolic'),
            ('transcription', 'Transcription', 'audio-input-microphone-symbolic'),
            ('controllers', 'Controllers', 'input-gaming-symbolic'),
        ):
            page = self.page()
            if name in SPECS:
                group = Adw.PreferencesGroup(title=title)
                if name == 'controllers':
                    group.set_description('Tap a joystick cap, release, then touch and hold to dictate.')
                for field in SPECS[name]:
                    group.add(self.field(field))
                page.append(group)
                if name == 'general' and not self.setup:
                    installed = updates.installed_info()
                    version = installed['version']
                    if installed['revision'] != 'unknown':
                        version += ' · ' + installed['revision'][:7]
                    about = Adw.PreferencesGroup(title='Updates', description='Installed: ' + version)
                    self.update_button = Gtk.Button(label='Check for updates', halign=Gtk.Align.START)
                    self.update_button.connect('clicked', self.check_updates)
                    about.add(self.update_button)
                    page.append(about)
                    spacer = Gtk.Box()
                    spacer.set_vexpand(True)
                    page.append(spacer)
                    self.uninstall_button = Gtk.Button(label='Uninstall…')
                    self.uninstall_button.add_css_class('destructive-action')
                    self.uninstall_button.set_halign(Gtk.Align.START)
                    self.uninstall_button.connect('clicked', self.confirm_uninstall)
                    page.append(self.uninstall_button)
            elif name == 'transcription':
                self.transcription_page(page)
            scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
            scroll.set_child(page)
            self.tabs.add_titled_with_icon(scroll, name, title, icon)
        if self.mode in ('general', 'transcription', 'controllers'):
            self.tabs.set_visible_child_name(self.mode)
        header = Adw.HeaderBar()
        header.set_title_widget(Adw.WindowTitle(title=self.window.get_title()))
        view = Adw.ToolbarView()
        view.add_top_bar(header)
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        switcher = Adw.ViewSwitcher(stack=self.tabs, policy=Adw.ViewSwitcherPolicy.WIDE)
        switcher.set_halign(Gtk.Align.CENTER)
        switcher.set_margin_top(6)
        switcher.set_margin_bottom(6)
        body.append(switcher)
        body.append(self.tabs)
        self.tabs.set_vexpand(True)
        self.status = Gtk.Label(xalign=0, wrap=True)
        self.status.set_margin_start(24)
        self.status.set_margin_end(24)
        self.status.set_margin_bottom(12)
        body.append(self.status)
        self.footer = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        self.footer.set_margin_start(24)
        self.footer.set_margin_end(24)
        self.footer.set_margin_bottom(20)
        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        self.footer.append(spacer)
        self.save_button = Gtk.Button(label='Install and start' if self.setup else 'Save settings')
        self.save_button.add_css_class('suggested-action')
        self.save_button.add_css_class('pill')
        self.save_button.connect('clicked', self.on_save)
        self.footer.append(self.save_button)
        body.append(self.footer)
        self.saved_values = self.collect_settings()
        for widget in self.fields:
            signal = 'notify::active' if widget.kind == 'bool' else 'notify::selected' if widget.kind == 'enum' else 'notify::text'
            widget.connect(signal, self.update_dirty)
        controls = {widget.key: widget for widget in self.fields}
        if 'VRBTN_CLEAR_RIGHT' in controls:
            controls['VRBTN_CLEAR_RIGHT'].connect('notify::selected', self.update_controller_options)
        self.update_controller_options()
        for widget in (self.backend_choice, self.language_choice):
            widget.connect('notify::selected', self.update_dirty)
        self.update_dirty()
        view.set_content(body)
        self.window.set_content(view)
        self.window.present()

    def page(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=24)
        for method in (box.set_margin_top, box.set_margin_bottom, box.set_margin_start, box.set_margin_end):
            method(24)
        return box

    def field(self, spec):
        key, label, kind = spec[:3]
        value = self.current.get(key)
        if kind == 'bool':
            widget = Adw.SwitchRow(title=label, active=truthy(value, spec[4]))
        elif kind == 'enum':
            options, default = spec[3:5]
            widget = Adw.ComboRow(title=label, model=Gtk.StringList.new([backend.option_label(option) for option in options]))
            widget.options = options
            widget.set_selected(options.index(value if value in options else default))
        else:
            widget = Adw.PasswordEntryRow(title=label) if kind == 'secret' else Adw.EntryRow(title=label)
            widget.set_text(value or (spec[4] if len(spec) > 4 else ''))
        widget.key, widget.kind = key, kind
        self.fields.append(widget)
        return widget

    def update_controller_options(self, *_):
        controls = {widget.key: widget for widget in self.fields}
        choice = controls.get('VRBTN_CLEAR_RIGHT')
        row = controls.get('VRBTN_CLEAR_B_SPACE')
        if choice is not None and row is not None:
            row.set_visible(choice.options[choice.get_selected()] == 'b')

    def transcription_page(self, page):
        choices = Adw.PreferencesGroup(title='Transcription', description='Choose where your speech is transcribed.')
        self.backend_modes = ['groq', 'local-fast', 'local-balanced']
        labels = ['Groq · Cloud', 'Local · Fast', 'Local · Balanced']
        mode = 'groq'
        if self.current.get('VOICE_BACKEND') == 'local':
            model = self.current.get('VOICE_MODEL', '')
            if model.endswith(('ggml-small.en.bin', 'ggml-small.bin')):
                mode = 'local-balanced'
            elif model.endswith(('ggml-base.en.bin', 'ggml-base.bin')):
                mode = 'local-fast'
            else:
                self.backend_modes.append('existing-local')
                labels.append('Local · Current model')
                mode = 'existing-local'
        self.backend_choice = Adw.ComboRow(title='Transcription path', model=Gtk.StringList.new(labels))
        self.backend_choice.set_selected(self.backend_modes.index(mode))
        if self.requested in self.backend_modes:
            self.backend_choice.set_selected(self.backend_modes.index(self.requested))
        choices.add(self.backend_choice)
        self.language_codes = ['auto', 'en'] + [code for code in backend.LANGUAGES if code != 'en']
        language_labels = ['Automatic detection', 'English'] + [backend.LANGUAGES[code] for code in self.language_codes[2:]]
        self.language_choice = Adw.ComboRow(title='Language', model=Gtk.StringList.new(language_labels), enable_search=True)
        language = self.language or self.current.get('VOICE_LANG', 'en').lower() or 'auto'
        self.language_choice.set_selected(self.language_codes.index(language) if language in self.language_codes else 0)
        choices.add(self.language_choice)
        page.append(choices)
        self.groq_group = Adw.PreferencesGroup(title='Groq', description='Cloud transcription requires internet access and a Groq API key.')
        self.api_key = self.field(('GROQ_API_KEY', 'Groq API key', 'secret'))
        self.groq_group.add(self.api_key)
        self.groq_group.add(self.field(('VOICE_CLEANUP', 'Clean up transcript', 'bool', None, True)))
        self.groq_group.add(self.field(('GROQ_USAGE_NOTIFY', 'In-headset notifications', 'bool', None, True)))
        self.groq_group.add(self.field(('VOICE_SLASH', 'Convert spoken slash to /', 'bool', None, False)))
        prompt_button = Gtk.Button(label='Edit cleanup prompt…')
        prompt_button.set_margin_top(14)
        prompt_button.set_margin_bottom(6)
        prompt_button.connect('clicked', self.open_prompt)
        self.groq_group.add(prompt_button)

        self.groq_group.add(Gtk.LinkButton(uri='https://console.groq.com/keys', label='Get a Groq API key'))
        page.append(self.groq_group)
        self.local_group = Adw.PreferencesGroup(title='Local transcription')
        self.local_info = Adw.ActionRow(title='Runs on your Frame', subtitle='')
        self.local_info.set_subtitle_lines(3)
        self.local_group.add(self.local_info)
        page.append(self.local_group)
        self.models_group = Adw.PreferencesGroup(title='Downloaded models', description='Delete unused models to free space. Selecting a deleted model again downloads it.')
        self.model_rows = []
        self.model_buttons = {}
        page.append(self.models_group)
        self.refresh_models()
        self.backend_choice.connect('notify::selected', self.update_backend)
        self.language_choice.connect('notify::selected', self.update_backend)
        self.update_backend()

    def update_backend(self, *_):
        mode = self.backend_modes[self.backend_choice.get_selected()]
        self.groq_group.set_visible(mode == 'groq')
        self.local_group.set_visible(mode != 'groq')
        detail = {
            'local-fast': 'Fast English dictation. Downloads a 142 MiB model on first use.',
            'local-balanced': 'More accurate English dictation. Downloads a 466 MiB model on first use.',
            'existing-local': 'Uses your currently configured local model.',
        }.get(mode, '')
        language = self.language_codes[self.language_choice.get_selected()]
        variant = 'English-only model.' if language == 'en' else 'Multilingual model for your language or automatic detection.'
        self.local_info.set_subtitle(detail.replace('English ', '') + ' ' + variant + ' After setup, transcription works offline. Verified downloads are reused.')

    def refresh_models(self):
        for row in self.model_rows: self.models_group.remove(row)
        self.model_rows = []
        self.model_buttons = {}
        try:
            models = backend.downloaded_models(Path.home())
            for mode, path in models:
                used = backend.model_in_use(Path.home(), path)
                size = path.stat().st_size / (1024 * 1024)
                label = backend.model_label(mode)
                row = Adw.ActionRow(title=label, subtitle=f'{size:.1f} MiB · ' + ('In use — save another path to delete' if used else 'Available to delete'))
                button = Gtk.Button(label='Delete…', valign=Gtk.Align.CENTER)
                button.add_css_class('destructive-action')
                button.set_sensitive(not used)
                button.connect('clicked', lambda _, selected=mode, title=label, mb=size: self.confirm_delete_model(selected, title, mb))
                row.add_suffix(button)
                self.models_group.add(row)
                self.model_rows.append(row)
                self.model_buttons[mode] = button
            if not models:
                row = Adw.ActionRow(title='No downloaded models', subtitle='Models downloaded by Frame Voice will appear here. Older shared models are kept separately.')
                self.models_group.add(row)
                self.model_rows.append(row)
        except (OSError, RuntimeError) as error:
            row = Adw.ActionRow(title='Models unavailable', subtitle=str(error))
            self.models_group.add(row)
            self.model_rows.append(row)

    def confirm_delete_model(self, mode, title, size):
        dialog = Adw.MessageDialog(transient_for=self.window, modal=True, heading=f'Delete {title} model?', body=f'Free {size:.1f} MiB by removing this download. Your settings and Groq API key will be kept. You can download the model again later.')
        dialog.add_response('cancel', 'Cancel')
        dialog.add_response('delete', 'Delete model')
        dialog.set_response_appearance('delete', Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.set_default_response('cancel')
        dialog.set_close_response('cancel')
        def response(_, answer):
            if answer != 'delete': return
            try:
                backend.delete_model(Path.home(), mode)
                self.status.set_text(f'{title} model deleted. Freed {size:.1f} MiB.')
            except (OSError, RuntimeError) as error:
                self.status.set_text(str(error))
            self.refresh_models()
        dialog.connect('response', response)
        dialog.present()

    def prompt_path(self):
        return Path(self.current.get('GROQ_PROMPT_FILE') or CONFIG.parent/'groq-prompt.txt').expanduser()

    def default_prompt(self):
        template = Path(__file__).parent/'groq-prompt.txt'
        if not template.exists(): template = Path(__file__).parent.parent/'assets/groq-prompt.txt'
        return template.read_text()

    def read_prompt(self):
        path = self.prompt_path()
        return path.read_text() if path.exists() else self.default_prompt()

    def open_prompt(self, *_):
        dialog = Adw.MessageDialog(transient_for=self.window, modal=True,
            heading='Groq cleanup prompt', body='Customize how transcripts are cleaned up. Apply here, then Save settings to use your changes.')
        self.prompt_editor = Gtk.TextView(wrap_mode=Gtk.WrapMode.WORD_CHAR)
        self.prompt_editor.set_top_margin(12)
        self.prompt_editor.set_bottom_margin(12)
        self.prompt_editor.set_left_margin(12)
        self.prompt_editor.set_right_margin(12)
        self.prompt_editor.get_buffer().set_text(self.prompt_text)
        scroll = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
        scroll.set_size_request(520, 280)
        scroll.set_child(self.prompt_editor)
        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        content.append(scroll)
        reset = Gtk.Button(label='Reset to default', halign=Gtk.Align.START)
        reset.connect('clicked', lambda *_: self.prompt_editor.get_buffer().set_text(self.default_prompt()))
        content.append(reset)
        dialog.set_extra_child(content)
        dialog.add_response('cancel', 'Cancel')
        dialog.add_response('apply', 'Apply')
        dialog.set_response_appearance('apply', Adw.ResponseAppearance.SUGGESTED)
        dialog.set_default_response('apply')
        dialog.set_close_response('cancel')
        def response(_, answer):
            if answer == 'apply':
                buffer = self.prompt_editor.get_buffer()
                self.prompt_text = buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), False)
                self.update_dirty()
        dialog.connect('response', response)
        dialog.present()

    def check_updates(self, *_):
        self.update_button.set_sensitive(False)
        def worker():
            result = updates.check()
            GLib.idle_add(self.show_updates, result)
        threading.Thread(target=worker, daemon=True).start()

    def show_updates(self, result):
        self.update_button.set_sensitive(True)
        status = result['status']
        heading = 'Update available' if status == 'available' else 'Up to date' if status == 'current' else 'Unable to check updates'
        body = 'Installed: ' + result['installed']['version']
        if 'latest' in result:
            body += '\nLatest: ' + result['latest']
        if status == 'unavailable':
            body += '\n' + result['message']
        elif status == 'available' and self.collect_settings() != self.saved_values:
            body += '\nSave settings before updating.'
        dialog = Adw.MessageDialog(transient_for=self.window, modal=True, heading=heading, body=body)
        dialog.add_response('close', 'Close')
        dialog.add_response('releases', 'Open releases')
        if status == 'available':
            dialog.add_response('update', 'Update…')
            dialog.set_response_appearance('update', Adw.ResponseAppearance.SUGGESTED)
            dialog.set_response_enabled('update', not self.busy and self.collect_settings() == self.saved_values)
        dialog.set_close_response('close')
        def response(_, choice):
            if choice == 'releases':
                subprocess.Popen(['xdg-open', result['url']])
            elif choice == 'update':
                subprocess.Popen(['bash', str(Path(__file__).with_name('update.sh')), '--gui'])
                self.quit()
        dialog.connect('response', response)
        dialog.present()
        return False

    def confirm_uninstall(self, *_):
        dialog = Adw.MessageDialog(transient_for=self.window, modal=True, heading='Uninstall Frame Voice?', body='Dictation will stop. The app, tray, cached data, and installation backups will be removed. Choose what to keep:')
        choices = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        self.keep_config = Gtk.CheckButton(label='Keep current configuration and API keys', active=True)
        self.keep_models = Gtk.CheckButton(label='Keep downloaded models', active=True)
        choices.append(self.keep_config)
        choices.append(self.keep_models)
        dialog.set_extra_child(choices)
        dialog.add_response('cancel', 'Cancel')
        dialog.add_response('uninstall', 'Uninstall')
        dialog.set_response_appearance('uninstall', Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.set_default_response('cancel')
        dialog.set_close_response('cancel')
        def response(dialog, answer):
            if answer == 'uninstall':
                self.begin_uninstall(self.keep_config.get_active(), self.keep_models.get_active())
        dialog.connect('response', response)
        dialog.present()

    def run_task(self, message, work, finished):
        self.busy = True
        self.tabs.set_sensitive(False)
        self.footer.set_sensitive(False)
        self.status.set_text(message)
        def worker():
            try:
                work()
                error = ''
            except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as exc:
                error = str(exc)
            GLib.idle_add(finished, error)
        threading.Thread(target=worker, daemon=True).start()

    def begin_uninstall(self, keep_config, keep_models):
        def work():
            command = [sys.executable, str(Path(__file__).with_name('local-install.py')), 'uninstall']
            if not keep_config: command.append('--remove-config')
            if not keep_models: command.append('--remove-models')
            result = subprocess.run(command, env=service_environment(), capture_output=True, text=True, timeout=180)
            if result.returncode: raise RuntimeError(result.stderr.strip() or 'Uninstall failed.')
        self.run_task('Uninstalling…', work, self.uninstall_finished)

    def uninstall_finished(self, error):
        self.busy = False
        self.tabs.set_sensitive(True)
        self.footer.set_sensitive(True)
        self.update_dirty()
        if error:
            self.status.set_text(error)
            return
        self.tabs.set_sensitive(False)
        self.uninstall_button.set_sensitive(False)
        dialog = Adw.MessageDialog(transient_for=self.window, modal=True, heading='Frame Voice uninstalled', body='The app and installation backups were removed. Your retention choices have been applied.')
        dialog.add_response('close', 'Close')
        dialog.connect('response', lambda *_: self.quit())
        dialog.present()

    def collect_settings(self):
        updates = {}
        for widget in self.fields:
            if widget.kind == 'bool':
                updates[widget.key] = '1' if widget.get_active() else '0'
            elif widget.kind == 'enum':
                updates[widget.key] = widget.options[widget.get_selected()]
            elif widget.get_text().strip():
                updates[widget.key] = widget.get_text().strip()
        if not updates.get('GROQ_API_KEY') and hasattr(self, 'saved_values') and self.saved_values.get('GROQ_API_KEY'):
            updates['GROQ_API_KEY'] = self.saved_values['GROQ_API_KEY']
        updates['prompt_text'] = self.prompt_text
        updates['backend'] = self.backend_modes[self.backend_choice.get_selected()]
        updates['language'] = self.language_codes[self.language_choice.get_selected()]
        return updates

    def update_dirty(self, *_):
        if hasattr(self, 'save_button'):
            self.save_button.set_sensitive(not self.busy and (self.setup or self.collect_settings() != self.saved_values))

    def on_save(self, *_):
        if self.busy: return
        updates = self.collect_settings()
        updates.pop('backend')
        updates.pop('language')
        prompt_text = updates.pop('prompt_text')
        if not prompt_text.strip() or len(prompt_text.encode()) > 32768:
            self.status.set_text('Cleanup prompt must contain 1–32768 bytes.')
            return
        try:
            maximum = float(updates['VOICE_MAX'])
            if not 0.1 <= maximum <= 120: raise ValueError()
        except ValueError:
            self.status.set_text('Maximum recording seconds must be between 0.1 and 120.')
            return
        mode = self.backend_modes[self.backend_choice.get_selected()]
        language = self.language_codes[self.language_choice.get_selected()]
        def work():
            prepared, _ = backend.prepare(Path.home(), mode, key_file=self.key_file, api_key=updates.get('GROQ_API_KEY'), allow_prompt=False, language=language)
            # Local transcription has no cloud cleanup step.
            if mode != 'groq':
                for key in ('VOICE_CLEANUP', 'GROQ_USAGE_NOTIFY', 'VOICE_SLASH'): updates.pop(key, None)
            prepared.update(updates)
            before = CONFIG.read_bytes() if CONFIG.exists() else None
            prompt_path = self.prompt_path()
            prompt_before = prompt_path.read_bytes() if prompt_path.exists() else None
            save(prepared)
            try:
                if prompt_text != self.saved_values.get('prompt_text'):
                    backend.write_private_text(prompt_path, prompt_text)
                result = subprocess.run([str(Path.home()/'.local/bin/frame-voice'), '--check'], env=service_environment(), capture_output=True, text=True, timeout=10)
                if result.returncode: raise RuntimeError(result.stderr.strip() or 'Configuration check failed.')
            except BaseException:
                if before is None: CONFIG.unlink(missing_ok=True)
                else: CONFIG.write_bytes(before)
                if prompt_before is None: prompt_path.unlink(missing_ok=True)
                else: prompt_path.write_bytes(prompt_before)
                raise
            if not self.setup:
                subprocess.run(['systemctl', '--user', 'restart', 'frame-voice-mixer', 'frame-voice'], env=service_environment(), check=True, capture_output=True, text=True, timeout=65)
        self.run_task('Saving…' if mode == 'groq' else 'Preparing local transcription… Downloading and verifying the model may take a few minutes.', work, self.save_finished)

    def save_finished(self, error):
        self.busy = False
        self.tabs.set_sensitive(True)
        self.footer.set_sensitive(True)
        self.update_dirty()
        if error: self.status.set_text(error)
        else:
            self.saved = True
            if self.setup:
                self.quit()
                return
            self.status.set_text('Settings saved. Dictation restarted.')
            self.current = read_env()
            self.saved_values = self.collect_settings()
            self.refresh_models()
        self.update_dirty()


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', nargs='?', default='general')
    parser.add_argument('--setup', action='store_true')
    parser.add_argument('--backend', choices=backend.MODES)
    parser.add_argument('--language')
    parser.add_argument('--groq-key-file', type=Path)
    args = parser.parse_args()
    GLib.set_prgname('dev.framevoice.setup' if args.setup else 'dev.framevoice.config')
    app = ConfigApp('transcription' if args.setup else args.mode, args.setup,
                    args.backend, args.language, args.groq_key_file)
    result = app.run([sys.argv[0]])
    return 1 if args.setup and not app.saved else result


if __name__ == '__main__':
    sys.exit(main())
