#!/bin/sh
# GUI entry point for an extracted release or a built source checkout.
set -eu
here=$(CDPATH= cd "$(dirname "$0")" && pwd)
if [ -f "$here/bin/frame-voice" ]; then
    exec "$here/install.sh" "$here/bin/frame-voice" --tray-binary "$here/bin/frame-voice-tray" --gui "$@"
fi
exec "$here/install.sh" --gui "$@"
