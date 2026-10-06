#!/bin/sh
# Steam Frame's LPASS ADC gains must be raised before capture.
set -eu
gain=${MIC_GAIN:-120}
card=${MIC_CARD:-0}
attempt=0
while [ "$attempt" -lt 30 ]; do
    if amixer -c "$card" sset VA_DEC0 "$gain" >/dev/null 2>&1; then
        amixer -c "$card" sset VA_DEC1 "$gain" >/dev/null 2>&1 || true
        exit 0
    fi
    attempt=$((attempt + 1))
    sleep 1
done
echo 'Frame Voice: microphone gain could not be configured.' >&2
exit 1
