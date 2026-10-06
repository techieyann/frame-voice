#!/bin/sh
# User-requested update through the same transactional stable installer.
set -eu
if [ "${1:-}" = --check ]; then
    shift
    here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
    exec python3 "$here/update-check.py" "$@"
fi
update_dir=$(mktemp -d)
trap 'rm -rf "$update_dir"' EXIT HUP INT TERM
curl -fsSL --connect-timeout 10 --max-time 60 \
    https://raw.githubusercontent.com/techieyann/frame-voice/main/get.sh \
    -o "$update_dir/get.sh"
sh "$update_dir/get.sh" "$@"
