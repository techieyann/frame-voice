#!/usr/bin/env python3
"""GTK4 configuration form for Frame Voice.

Shows real on/off switches (never 0/1), dropdowns and text fields pre-filled with
the current values, so users never see env-var names and always see what is set.
Writes ~/.config/frame-voice/env and restarts the service.

usage: config-gui.py [general|controllers]
"""
import os
import subprocess
import sys
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk  # noqa: E402

CONFIG = (
    Path(os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")))
    / "frame-voice"
    / "env"
)

# key, label, kind, options, default
SPECS = {
    "general": [
        ("GROQ_API_KEY", "Groq API key", "secret"),
        ("VOICE_INJECT", "Text injection", "enum", ["paste", "type"], "type"),
        ("VOICE_BEEP", "Start/stop beeps", "bool", None, True),
        ("VOICE_CLEANUP", "Clean up transcript", "bool", None, True),
        ("VOICE_TRAILING", "Separator after dictation", "enum", ["space", "none"], "space"),
        ("VOICE_NOTIFY", "In-headset notifications", "bool", None, True),
        ("VOICE_MAX", "Max recording seconds", "text", None, "30"),
    ],
    "controllers": [
        ("VRBTN_DICTATE_RIGHT", "Right hand dictation", "bool", None, True),
        ("VRBTN_SUBMIT_RIGHT", "Right submit button", "enum", ["a", "b", "x", "y", "none"], "a"),
        ("VRBTN_CLEAR_RIGHT", "Right clear button", "enum", ["a", "b", "x", "y", "none"], "b"),
        ("VRBTN_DICTATE_LEFT", "Left hand dictation", "bool", None, True),
        ("VRBTN_SUBMIT_LEFT", "Left submit button", "enum",
         ["dpad_up", "dpad_down", "dpad_left", "dpad_right", "none"], "dpad_right"),
        ("VRBTN_CLEAR_LEFT", "Left clear button", "enum",
         ["dpad_up", "dpad_down", "dpad_left", "dpad_right", "none"], "dpad_left"),
    ],
}


def read_env():
    values = {}
    try:
        for line in CONFIG.read_text().splitlines():
            text = line.strip()
            if text and not text.startswith("#") and "=" in text:
                key, value = text.split("=", 1)
                values[key.strip()] = value.strip()
    except FileNotFoundError:
        pass
    return values


def truthy(value, default):
    if value is None:
        return default
    return value.strip().lower() in ("1", "true", "yes", "on")


def save(updates):
    lines = CONFIG.read_text().splitlines() if CONFIG.exists() else []
    seen = set()
    out = []
    for line in lines:
        text = line.strip()
        if text and not text.startswith("#") and "=" in text:
            key = text.split("=", 1)[0].strip()
            if key in updates:
                out.append(f"{key}={updates[key]}")
                seen.add(key)
                continue
        out.append(line)
    for key, value in updates.items():
        if key not in seen:
            out.append(f"{key}={value}")
    CONFIG.parent.mkdir(parents=True, exist_ok=True)
    CONFIG.write_text("\n".join(out).rstrip("\n") + "\n")
    os.chmod(CONFIG, 0o600)


class ConfigApp(Gtk.Application):
    def __init__(self, mode):
        super().__init__(application_id=f"dev.framevoice.config.{mode}")
        self.mode = mode if mode in SPECS else "general"
        self.fields = []

    def do_activate(self):
        current = read_env()
        title = "Frame Voice — controllers" if self.mode == "controllers" else "Frame Voice settings"
        window = Gtk.ApplicationWindow(application=self, title=title)
        window.set_default_size(440, -1)

        grid = Gtk.Grid(column_spacing=14, row_spacing=10)
        grid.set_margin_top(18)
        grid.set_margin_bottom(6)
        grid.set_margin_start(18)
        grid.set_margin_end(18)

        for row, spec in enumerate(SPECS[self.mode]):
            key, label, kind = spec[0], spec[1], spec[2]
            value = current.get(key)
            name = Gtk.Label(label=label, xalign=0)
            grid.attach(name, 0, row, 1, 1)

            if kind == "bool":
                default = spec[4] if len(spec) > 4 else True
                widget = Gtk.Switch()
                widget.set_active(truthy(value, default))
                widget.set_halign(Gtk.Align.START)
            elif kind == "enum":
                options = spec[3]
                default = spec[4] if len(spec) > 4 else options[0]
                widget = Gtk.ComboBoxText()
                for option in options:
                    widget.append_text(option)
                widget.set_active(options.index(value if value in options else default))
            elif kind == "secret":
                widget = Gtk.Entry()
                widget.set_visibility(False)
                if value:
                    widget.set_text(value)
                widget.set_hexpand(True)
            else:
                default = spec[4] if len(spec) > 4 else ""
                widget = Gtk.Entry()
                widget.set_text(value if value is not None else default)
                widget.set_hexpand(True)

            widget.key = key
            widget.kind = kind
            grid.attach(widget, 1, row, 1, 1)
            self.fields.append(widget)

        cancel = Gtk.Button(label="Cancel")
        cancel.connect("clicked", lambda *_: self.quit())
        save_button = Gtk.Button(label="Save")
        save_button.add_css_class("suggested-action")
        save_button.connect("clicked", lambda *_: self.on_save())
        buttons = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        buttons.set_halign(Gtk.Align.END)
        buttons.set_margin_bottom(16)
        buttons.set_margin_end(18)
        buttons.append(cancel)
        buttons.append(save_button)

        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        outer.append(grid)
        outer.append(buttons)
        window.set_child(outer)
        window.present()

    def on_save(self):
        updates = {}
        for widget in self.fields:
            if widget.kind == "bool":
                updates[widget.key] = "1" if widget.get_active() else "0"
            elif widget.kind == "enum":
                text = widget.get_active_text()
                if text is not None:
                    updates[widget.key] = text
            elif widget.kind == "secret":
                text = widget.get_text()
                if text:  # empty keeps the existing key
                    updates[widget.key] = text
            else:
                text = widget.get_text().strip()
                if text:
                    updates[widget.key] = text
        save(updates)
        uid = os.getuid()
        env = dict(os.environ)
        env["DBUS_SESSION_BUS_ADDRESS"] = f"unix:path=/run/user/{uid}/bus"
        env["XDG_RUNTIME_DIR"] = f"/run/user/{uid}"
        try:
            subprocess.run(["systemctl", "--user", "restart", "frame-voice"],
                           env=env, timeout=8, check=False)
        except OSError:
            pass
        self.quit()


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "general"
    return ConfigApp(mode).run([sys.argv[0]])


if __name__ == "__main__":
    sys.exit(main())
