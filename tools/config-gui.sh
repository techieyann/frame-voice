#!/usr/bin/env bash
# Entry point for the Frame Voice config GUI. Prefers the GTK4 form (real on/off
# switches and pre-filled current values); falls back to the zenity form.
# usage: config-gui.sh [general|controllers]
set -u
here="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if command -v python3 >/dev/null 2>&1 &&
    python3 -c "import gi; gi.require_version('Gtk','4.0')" >/dev/null 2>&1; then
    exec python3 "$here/config-gui.py" "$@"
fi
exec bash "$here/config-gui-zenity.sh" "$@"
