#!/bin/sh
# Native Frame build; the tray is independently reusable and has its own lockfile.
set -eu
cd "$(CDPATH= cd "$(dirname "$0")" && pwd)"
[ "$(uname -s)" = Linux ] && [ "$(uname -m)" = aarch64 ] || {
    echo 'Build on an aarch64 SteamOS Frame with Rust and a C++ compiler.' >&2; exit 1;
}
: "${OPENVR_INCLUDE_DIR:=/opt/steamvr/tools/hellovr_vulkan_linux/src/openvr/headers}"
export OPENVR_INCLUDE_DIR
cargo build --locked --release --features openvr
# First tray build resolves dependencies; keep the resulting lockfile for later builds.
if [ ! -f tray/Cargo.lock ]; then cargo generate-lockfile --manifest-path tray/Cargo.toml; fi
cargo build --locked --release --manifest-path tray/Cargo.toml
printf '%s\n' 'Built daemon/helper and tray. Run ./install.sh, then configure and activate.'
