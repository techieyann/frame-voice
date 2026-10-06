#!/usr/bin/env bash
# Entry point for the Frame Voice config GUI. Prefers the dark libadwaita form
# switches and pre-filled current values); falls back to the zenity form.
# usage: config-gui.sh [general|transcription|controllers|uninstall]
set -u
here="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# A separate service lets the window finish uninstalling after the tray stops.
if [ "${1:-}" != --window ]; then
    uid=$(id -u)
    gui_env=()
    for name in DISPLAY WAYLAND_DISPLAY XAUTHORITY XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS XDG_CONFIG_HOME XDG_DATA_HOME XDG_DATA_DIRS XDG_CURRENT_DESKTOP; do
        if [ -v "$name" ]; then gui_env+=("$name=${!name}"); fi
    done
    exec env DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${uid}/bus" XDG_RUNTIME_DIR="/run/user/${uid}" \
        systemd-run --user --collect --quiet --unit="frame-voice-config-$$" --property=Type=exec \
        /usr/bin/env "${gui_env[@]}" GSK_RENDERER=cairo bash "$here/config-gui.sh" --window "$@"
fi
shift
if command -v python3 >/dev/null 2>&1 &&
    python3 -c "import gi; gi.require_version('Gtk','4.0'); gi.require_version('Adw','1')" >/dev/null 2>&1; then
    exec python3 "$here/config-gui.py" "$@"
fi
exec bash "$here/config-gui-zenity.sh" "$@"
