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

mode="${1:-}"
if [ -z "$mode" ]; then
    mode=$(zenity --list --radiolist --title="Frame Voice settings" \
        --text="Choose a settings section." --column="" --column="Section" \
        TRUE General FALSE Transcription FALSE Controllers FALSE Uninstall) || exit 0
    mode=$(printf '%s' "$mode" | tr '[:upper:]' '[:lower:]')
fi
if [ "$mode" = uninstall ]; then
    keep=$(zenity --list --checklist --title="Uninstall Frame Voice" \
        --text="Remove Frame Voice, its tray, app data, and backups." \
        --separator='|' --column="" --column="Option" TRUE "Keep current configuration" TRUE "Keep downloaded models" \
        --ok-label="Uninstall…") || exit 0
    flags=()
    if [[ "$keep" == *"Keep current configuration"* ]]; then
        detail="Your saved configuration and API key will be kept."
    else
        detail="Your Frame Voice configuration and API key will be deleted."
        flags=(--remove-config)
    fi
    if [[ "$keep" == *"Keep downloaded models"* ]]; then
        detail="$detail Downloaded models will be kept."
    else
        detail="$detail Downloaded models will be deleted."
        flags+=(--remove-models)
    fi
    zenity --question --title="Uninstall Frame Voice?" \
        --text="Dictation will stop and the tray will be removed. $detail" \
        --ok-label="Uninstall" --cancel-label="Cancel" --default-cancel || exit 0
    here="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
    uid=$(id -u)
    if error=$(env DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${uid}/bus" \
        XDG_RUNTIME_DIR="/run/user/${uid}" python3 "$here/local-install.py" uninstall "${flags[@]}" 2>&1); then
        zenity --info --title="Frame Voice" --text="Frame Voice uninstalled. $detail"
    else
        zenity --error --title="Frame Voice" --text="$error"
    fi
    exit 0
fi
if [ "$mode" = transcription ]; then
    selected=$(zenity --list --radiolist --title="Transcription" \
        --text="Choose where speech is transcribed. Local models download on first use." \
        --column="" --column="Mode" --column="Description" --hide-column=2 --print-column=2 \
        TRUE groq "Groq · Cloud" FALSE local-fast "Local · Fast (142 MiB)" FALSE local-balanced "Local · Balanced (466 MiB)" FALSE manage-models "Manage downloaded models…") || exit 0
    here="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
    if ! error=$(python3 "$here/setup-backend.py" "$selected" 2>&1); then
        zenity --error --title="Transcription" --text="$error"
        exit 1
    fi
    [ "$selected" != manage-models ] || exit 0
    uid=$(id -u)
    env DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${uid}/bus" XDG_RUNTIME_DIR="/run/user/${uid}" \
        systemctl --user restart frame-voice-mixer frame-voice
    zenity --info --title="Frame Voice" --text="Transcription path saved."
    exit 0
fi
env_file="$HOME/.config/frame-voice/env"
mkdir -p "$(dirname "$env_file")"
touch "$env_file"
chmod 600 "$env_file"

cur() { sed -nE "s/^$1=(.*)$/\1/p" "$env_file" 2>/dev/null | tail -1; }

friendly() {
    case "$1" in
        a|b|x|y) printf '%s' "${1^^}" ;;
        dpad_up) printf 'D-pad Up' ;; dpad_down) printf 'D-pad Down' ;;
        dpad_left) printf 'D-pad Left' ;; dpad_right) printf 'D-pad Right' ;;
        none) printf 'None' ;; *) printf '%s' "$1" ;;
    esac
}
combo() { # $1 = current, rest = options -> "keep current (X)" then the options
    local current="$1"; shift
    local out="keep current ($(friendly "${current:-unset}"))" item
    for item in "$@"; do
        item=$(friendly "$item")
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
    local value="$2"
    case "$value" in
        A|B|X|Y|None) value="${value,,}" ;;
        "D-pad "*) value="dpad_${value##* }"; value="${value,,}" ;;
    esac
    set_kv "$1" "$value"
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
        --add-combo="Text injection" --combo-values="$(combo "$(cur VOICE_INJECT)" paste type)" \
        --add-combo="Start/stop beeps" --combo-values="$(combo "$(cur VOICE_BEEP)" 1 0)" \
        --add-combo="Separator after dictation" --combo-values="$(combo "$(cur VOICE_TRAILING)" space none)" \
        --add-entry="Max recording seconds  (blank = keep current)") || exit 0
    IFS="$sep" read -r inject beep trailing max_seconds <<<"$result"
    apply VOICE_INJECT "$inject"
    apply VOICE_BEEP "$beep"
    apply VOICE_TRAILING "$trailing"
    [ -n "$max_seconds" ] && set_kv VOICE_MAX "$max_seconds"
    note="Saved. Frame Voice restarted with the new settings."
fi

# Apply immediately (outer user-manager bus, workable from the nested session).
uid=$(id -u)
DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/${uid}/bus" \
    XDG_RUNTIME_DIR="/run/user/${uid}" \
    systemctl --user restart frame-voice >/dev/null 2>&1 || true
zenity --info --title="Frame Voice" --text="$note" 2>/dev/null || true
