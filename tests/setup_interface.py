"""Frontend selection and terminal navigation; no terminal/service/network changes."""
import curses
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

PROJECT = Path(__file__).resolve().parent.parent
def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), PROJECT/'tools'/f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
ui, backend = load('setup-interface'), load('setup-backend')


class Screen:
    def __init__(self, keys): self.keys = iter(keys)
    def erase(self): pass
    def getmaxyx(self): return 24, 100
    def addnstr(self, *args): pass
    def refresh(self): pass
    def getch(self): return next(self.keys)


class InterfaceTests(unittest.TestCase):
    def test_tui_language_selection_and_install_options(self):
        with tempfile.TemporaryDirectory() as home, patch.object(curses, 'curs_set'):
            screen = Screen([curses.KEY_DOWN, 10, curses.KEY_HOME, 10, curses.KEY_END, 10])
            mode, language, key, options = ui.tui_options(screen, backend, Path(home), 'local-fast', 'en', None)
        self.assertEqual(mode, 'local-fast')
        self.assertEqual(language, 'auto')
        self.assertEqual(key, '')
        self.assertNotIn('VOICE_CLEANUP', options)
        self.assertEqual(options['VRBTN_SUBMIT_RIGHT'], 'a')

    def test_tui_cancellation_is_explicit(self):
        with self.assertRaises(RuntimeError): ui.choose_screen(Screen([27]), 'Setup', ['Option'])

    def test_explicit_agent_backend_bypasses_desktop_detection(self):
        provider = Mock()
        with patch.object(ui, 'desktop_environment', side_effect=AssertionError('unexpected GUI probe')):
            ui.configure(provider, Path('/test'), requested='groq', language='de')
        provider.configure.assert_called_once_with(Path('/test'), 'groq', None, language='de')

    def test_gui_cancellation_precedes_input_permissions(self):
        provider = Mock()
        with patch.object(ui, 'desktop_environment', return_value={'DISPLAY': ':2'}), \
             patch.object(ui.subprocess, 'run', return_value=Mock(returncode=1)):
            with self.assertRaises(RuntimeError): ui.configure(provider, Path('/test'), ui='gui')
        provider.input_permissions.assert_not_called()


if __name__ == '__main__': unittest.main()
