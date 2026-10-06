#!/usr/bin/env bash
# GUI configuration form for Frame Voice (zenity, shipped on SteamOS).
#
# Presents native dialogs with labelled inputs so the user never has to know
# env-var names. Answers merge into ~/.config/frame-voice/env. Leave a field as
# "keep current" / blank to keep its present value.
#
# usage: config-gui.sh [general|controllers]
# Run inside the nested Desktop session (the tray does this). Safe to cancel.
set -u

mode="${1:-general}"
env_file="${XDG_CONFIG_HOME:-$HOME/.config}/frame-voice/env"
mkdir -p "$(dirname "$env_file")"
touch "$env_file"
chmod 600 "$env_file"

cur() { sed -nE "s/^$1=(.*)$/\1/p" "$env_file" 2>/dev/null | tail -1; }

combo() { # $1 = current, rest = options -> "keep current (X)" then the options
    local current="$1"; shift
    local out="keep current (${current:-unset})" item
    for item in "$@"; do
        case "|$out|" in *"|$item|"*) continue ;; esac
        out="$out|$item"
    done
    printf '%s' "$out"
}

set_kv() { # $1 = key, $2 = value
    local key="$1" value="$2" tmp
    tmp=$(mktemp)
    grep -v "^${key}=" "$env_file" 2>/dev/null >"$tmp" || true
    printf '%s=%s\n' "$key" "$value" >>"$tmp"
    install -m 600 "$tmp" "$env_file"
    rm -f "$tmp"
}
apply() { # $1 = key, $2 = chosen value; "keep current"/blank = no change
    case "$2" in "" | "keep current"*) return 0 ;; esac
    set_kv "$1" "$2"
}

sep=$'\x1f'

if [ "$mode" = controllers ]; then
    result=$(zenity --forms --title="Frame Voice — controllers" \
        --text="Choose the submit/clear buttons and enable or disable each hand. Leave a field to keep it." \
        --separator="$sep" \
        --add-combo="Right hand (thumbstick dictation)" --combo-values="$(combo "$(cur VRBTN_DICTATE_RIGHT)" 1 0)" \
        --add-combo="Right submit button" --combo-values="$(combo "$(cur VRBTN_SUBMIT_RIGHT)" a b x y none)" \
        --add-combo="Right clear button" --combo-values="$(combo "$(cur VRBTN_CLEAR_RIGHT)" a b x y none)" \
        --add-combo="Left hand (thumbstick dictation)" --combo-values="$(combo "$(cur VRBTN_DICTATE_LEFT)" 1 0)" \
        --add-combo="Left submit button" --combo-values="$(combo "$(cur VRBTN_SUBMIT_LEFT)" dpad_up dpad_down dpad_left dpad_right none)" \
        --add-combo="Left clear button" --combo-values="$(combo "$(cur VRBTN_CLEAR_LEFT)" dpad_up dpad_down dpad_left dpad_right none)") || exit 0
    IFS="$sep" read -r r_dictate r_submit r_clear l_dictate l_submit l_clear <<<"$result"
    apply VRBTN_DICTATE_RIGHT "$r_dictate"
    apply VRBTN_SUBMIT_RIGHT "$r_submit"
    apply VRBTN_CLEAR_RIGHT "$r_clear"
    apply VRBTN_DICTATE_LEFT "$l_dictate"
    apply VRBTN_SUBMIT_LEFT "$l_submit"
    apply VRBTN_CLEAR_LEFT "$l_clear"
    note="Saved. Frame Voice restarted with the new controller mapping."
else
    result=$(zenity --forms --title="Frame Voice" \
        --text="Configure Frame Voice. Leave a field as-is to keep its current value." \
        --separator="$sep" \
        --add-password="Groq API key  (blank = keep current)" \
        --add-combo="Text injection" --combo-values="$(combo "$(cur VOICE_INJECT)" paste type)" \
        --add-combo="Start/stop beeps" --combo-values="$(combo "$(cur VOICE_BEEP)" 1 0)" \
        --add-combo="Clean up transcript" --combo-values="$(combo "$(cur VOICE_CLEANUP)" 1 0)" \
        --add-combo="Separator after dictation" --combo-values="$(combo "$(cur VOICE_TRAILING)" space none)" \
        --add-combo="In-headset notifications" --combo-values="$(combo "$(cur VOICE_NOTIFY)" 1 0)" \
        --add-entry="Max recording seconds  (blank = keep current)") || exit 0
    IFS="$sep" read -r api_key inject beep cleanup trailing notify max_seconds <<<"$result"
    [ -n "$api_key" ] && set_kv GROQ_API_KEY "$api_key"
    apply VOICE_INJECT "$inject"
    apply VOICE_BEEP "$beep"
    apply VOICE_CLEANUP "$cleanup"
    apply VOICE_TRAILING "$trailing"
    apply VOICE_NOTIFY "$notify"
    [ -n "$max_seconds" ] && set_kv VOICE_MAX "$max_seconds"
    note="Saved. Frame Voice restarted with the new settings."
fi

# Apply immediately (outer user-manager bus, workable from the nested session).
uid=$(id -u)
DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${uid}/bus" \
    XDG_RUNTIME_DIR="/run/user/${uid}" \
    systemctl --user restart frame-voice >/dev/null 2>&1 || true
zenity --info --title="Frame Voice" --text="$note" 2>/dev/null || true
