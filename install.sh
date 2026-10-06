#!/bin/sh
# Local install only; no downloads or GitHub release machinery.
set -eu
script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
exec python3 "$script_dir/tools/local-install.py" install "$@"
