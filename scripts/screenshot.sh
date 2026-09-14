#!/usr/bin/env bash
#
# Headless screenshot of the Terminus UI, for developing the front end without
# a visible desktop.
#
#   scripts/screenshot.sh                     # capture the current UI
#   scripts/screenshot.sh --size 1440x900     # pick a viewport
#   scripts/screenshot.sh --send 'ctrl+shift+e'   # press keys first
#   scripts/screenshot.sh --hot-config 'margin = [60, 60, 60, 60]'
#                                             # capture, edit the config, capture
#                                             # again and report the pixel diff
#
# How it works: the app is an X11 client, so it can run against Xvfb (a virtual
# display with a real framebuffer). xwd then reads that framebuffer back and
# ImageMagick turns it into a PNG. WAYLAND_DISPLAY must be cleared or the app
# picks WSLg's Wayland compositor instead and never touches Xvfb — in that case
# the X display stays empty and every capture comes back solid black.
#
# Options:
#   --size WxH          capture size (default 1600x1000)
#   --out PATH          output PNG (default .dev/shots/<timestamp>.png)
#   --wait SECONDS      settle time before capturing (default 12)
#   --default-config    ignore .dev/config, use rio's defaults
#   --send KEYS         xdotool key spec to send before capturing (repeatable)
#   --type TEXT         type TEXT then press Return, before capturing (repeatable)
#   --no-resize         keep the window at its configured size
#   --hot-config TEXT   add TEXT to .dev/config/config.toml, wait, capture again
#   --keep              leave Xvfb + the app running after the capture
#   -- <args>           pass the rest through to rio
#
# Input goes through XTEST (xdotool without --window, after an XSetInputFocus):
# the app's winit backend ignores XSendEvent, which is what `xdotool key
# --window …` uses, so synthetic keys only land when aimed at the focused
# window.
#
# Env: TERMINUS_SCREENSHOT_DISPLAY (default :99), TERMINUS_DEV_NO_NIX=1 to skip
# the devshell re-exec.
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_PATH="${BASH_SOURCE[0]}"
DEV_DIR="$ROOT/.dev"
SHOT_DIR="$DEV_DIR/shots"
CONFIG_DIR="$DEV_DIR/config"
CONFIG_FILE="$CONFIG_DIR/config.toml"
TOOLS_ENV="$DEV_DIR/screenshot-tools.env"

SIZE="1600x1000"
OUT=""
WAIT_S=12
USE_CONFIG=1
KEEP=0
RESIZE=1
declare -a SEND=()
declare -a TYPE=()
HOT_CONFIG=""
declare -a APP_ARGS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --size) SIZE="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        --wait) WAIT_S="$2"; shift 2 ;;
        --default-config) USE_CONFIG=0; shift ;;
        --send) SEND+=("$2"); shift 2 ;;
        --type) TYPE+=("$2"); shift 2 ;;
        --no-resize) RESIZE=0; shift ;;
        --hot-config) HOT_CONFIG="$2"; shift 2 ;;
        --keep) KEEP=1; shift ;;
        --) shift; APP_ARGS=("$@"); break ;;
        -h|--help) sed -n '2,40p' "$SCRIPT_PATH" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "screenshot.sh: unknown option '$1'" >&2; exit 2 ;;
    esac
done

if [[ "${TERMINUS_DEV_NO_NIX:-0}" != "1" && "${TERMINUS_DEV_SHELL:-0}" != "1" ]]; then
    command -v nix >/dev/null 2>&1 || { echo "screenshot.sh: nix not found" >&2; exit 1; }
    # Rebuild the argument list as an array. Building it as a string and letting
    # it word-split mangles any value containing a space ("--type 'echo hi'"),
    # so nothing here goes through $(...) interpolation.
    reexec=(nix develop "$ROOT" --command env TERMINUS_DEV_SHELL=1 bash "$SCRIPT_PATH"
            --size "$SIZE" --wait "$WAIT_S")
    [[ -n "$OUT" ]] && reexec+=(--out "$OUT")
    [[ "$USE_CONFIG" == "0" ]] && reexec+=(--default-config)
    [[ "$RESIZE" == "0" ]] && reexec+=(--no-resize)
    [[ "$KEEP" == "1" ]] && reexec+=(--keep)
    [[ -n "$HOT_CONFIG" ]] && reexec+=(--hot-config "$HOT_CONFIG")
    for k in "${SEND[@]+"${SEND[@]}"}"; do reexec+=(--send "$k"); done
    for t in "${TYPE[@]+"${TYPE[@]}"}"; do reexec+=(--type "$t"); done
    reexec+=(--)
    exec "${reexec[@]}" ${APP_ARGS[@]+"${APP_ARGS[@]}"}
fi

# --------------------------------------------------------------- toolchain ----
# Xvfb/xwd/imagemagick/xdotool are not installed system-wide here; resolve them
# from nixpkgs once and cache the paths.
if [[ ! -s "$TOOLS_ENV" ]]; then
    echo "screenshot.sh: resolving capture toolchain from nixpkgs (first run)…" >&2
    {
        for pkg in xvfb xorg.xwd xorg.xwininfo xdotool imagemagick; do
            path="$(nix build --no-link --print-out-paths "nixpkgs#$pkg" 2>/dev/null | tail -1 || true)"
            [[ -n "$path" ]] || { echo "screenshot.sh: could not resolve nixpkgs#$pkg" >&2; exit 1; }
            printf 'export TERMINUS_TOOL_%s=%q\n' "${pkg//./_}" "$path"
        done
    } >"$TOOLS_ENV"
fi
# shellcheck disable=SC1090
. "$TOOLS_ENV"
TOOL_PATH="$TERMINUS_TOOL_xvfb/bin:$TERMINUS_TOOL_xorg_xwd/bin:$TERMINUS_TOOL_xorg_xwininfo/bin"
TOOL_PATH="$TOOL_PATH:$TERMINUS_TOOL_xdotool/bin:$TERMINUS_TOOL_imagemagick/bin"
export PATH="$TOOL_PATH:$PATH"

DISP="${TERMINUS_SCREENSHOT_DISPLAY:-:99}"
mkdir -p "$SHOT_DIR" "$CONFIG_DIR"
# shellcheck disable=SC1091
[[ -s "$DEV_DIR/gpu-env.sh" ]] && . "$DEV_DIR/gpu-env.sh"

BIN="$ROOT/target/debug/rio"
if [[ ! -x "$BIN" ]]; then
    echo "screenshot.sh: $BIN missing — run scripts/dev.sh --once first" >&2
    exit 1
fi

if [[ ! -f "$CONFIG_FILE" && "$USE_CONFIG" == "1" && -f "$ROOT/dev/config/config.toml" ]]; then
    cp "$ROOT/dev/config/config.toml" "$CONFIG_FILE"
fi

pick_window() {
    xwininfo -root -tree 2>/dev/null \
        | grep -E '"Rio"|"rio"' | grep -oE '0x[0-9a-f]+' | head -1
}

capture() { # capture <path> — grab the app window, fall back to the whole root
    local out="$1" win
    win="$(pick_window)"
    if [[ -n "$win" ]] && xwd -silent -id "$win" -out "${out%.png}.xwd" 2>/dev/null; then
        magick "${out%.png}.xwd" "$out" 2>/dev/null || convert "${out%.png}.xwd" "$out"
    else
        echo "screenshot.sh: no rio window found, capturing the root instead" >&2
        xwd -root -silent -out "${out%.png}.xwd" || return 1
        magick "${out%.png}.xwd" "$out" 2>/dev/null || convert "${out%.png}.xwd" "$out"
    fi
    rm -f "${out%.png}.xwd"
}

# ----------------------------------------------------------------- lifecycle ----
rm -f "/tmp/.X${DISP#:}-lock"
Xvfb "$DISP" -screen 0 "${SIZE}x24" -nolisten tcp >"$DEV_DIR/logs/xvfb.log" 2>&1 &
XVFB_PID=$!
sleep 2

# xwininfo/xwd/xdotool all need the display. Export it for this shell too, not
# just for the app: without it every window lookup silently finds nothing.
export DISPLAY="$DISP"

app_env=(DISPLAY="$DISP" RIO_LOG_LEVEL="${RIO_LOG_LEVEL:-info}")
[[ "$USE_CONFIG" == "1" ]] && app_env+=(RIO_CONFIG_HOME="$CONFIG_DIR")
(
    cd "$ROOT"
    exec env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET "${app_env[@]}" "$BIN" ${APP_ARGS[@]+"${APP_ARGS[@]}"}
) >"$DEV_DIR/logs/screenshot-app.log" 2>&1 &
APP_PID=$!

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    if [[ "$KEEP" != "1" ]]; then
        kill "$APP_PID" "$XVFB_PID" 2>/dev/null
        wait "$APP_PID" 2>/dev/null
    else
        echo "  kept: Xvfb pid $XVFB_PID (DISPLAY=$DISP), app pid $APP_PID"
    fi
    exit "$status"
}
trap cleanup EXIT INT TERM

echo "screenshot.sh: waiting ${WAIT_S}s for the window…"
sleep "$WAIT_S"

if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "screenshot.sh: the app exited early — see $DEV_DIR/logs/screenshot-app.log" >&2
    tail -5 "$DEV_DIR/logs/screenshot-app.log" >&2
    exit 1
fi

# Resize the window to the requested viewport. Rio opens at its configured size
# and there is no window manager here, so the size has to be pushed onto the
# window explicitly — otherwise the capture is always 800x490 no matter what
# --size says.
if [[ "$RESIZE" == "1" ]]; then
    win="$(pick_window)"
    if [[ -n "$win" ]]; then
        xdotool windowsize "$win" "${SIZE%x*}" "${SIZE#*x}" 2>/dev/null || true
        sleep 3
    else
        echo "screenshot.sh: no rio window to resize (capture will use the app's own size)" >&2
    fi
fi

# Keys and text go in through XTEST. windowfocus is XSetInputFocus, which the
# app honours; XSendEvent (xdotool's --window flag) is silently dropped.
if [[ ${#SEND[@]} -gt 0 || ${#TYPE[@]} -gt 0 ]]; then
    win="$(pick_window)"
    if [[ -z "$win" ]]; then
        echo "screenshot.sh: no rio window — cannot send input" >&2
    else
        xdotool windowfocus "$win" 2>/dev/null || true
        sleep 1
        for keys in "${SEND[@]+"${SEND[@]}"}"; do
            xdotool key --clearmodifiers $keys 2>/dev/null || true
            echo "  sent keys: $keys"
            sleep 1.5
        done
        for text in "${TYPE[@]+"${TYPE[@]}"}"; do
            xdotool type --delay 60 "$text" 2>/dev/null || true
            xdotool key Return 2>/dev/null || true
            echo "  typed: $text"
            sleep 2
        done
    fi
fi

OUT="${OUT:-$SHOT_DIR/$(date +%H%M%S)-$(date +%N | cut -c1-3).png}"
capture "$OUT" || exit 1
echo "  captured $OUT ($(identify -format '%wx%h, %k colors' "$OUT"))"

if [[ -n "$HOT_CONFIG" ]]; then
    BEFORE="${OUT%.png}-before.png"
    AFTER="${OUT%.png}-after.png"
    mv "$OUT" "$BEFORE"
    # Replace the key in place when it already exists, otherwise insert it at the
    # top of the file. Blindly appending would land the line inside whichever
    # [table] happens to be last and silently change its meaning.
    hot_key="$(printf '%s' "$HOT_CONFIG" | cut -d= -f1 | tr -d '[:space:]')"
    if grep -qE "^[[:space:]]*${hot_key}[[:space:]]*=" "$CONFIG_FILE"; then
        esc="$(printf '%s' "$HOT_CONFIG" | sed 's/[&\\]/\\&/g')"
        sed -i "0,/^[[:space:]]*${hot_key}[[:space:]]*=.*/s//${esc}/" "$CONFIG_FILE"
    else
        printf '%s\n' "$HOT_CONFIG" | cat - "$CONFIG_FILE" >"$CONFIG_FILE.tmp"
        mv "$CONFIG_FILE.tmp" "$CONFIG_FILE"
    fi
    echo "  config now: $HOT_CONFIG"
    sleep 4
    if ! kill -0 "$APP_PID" 2>/dev/null; then
        echo "  the app DIED after the config change" >&2
        exit 1
    fi
    capture "$AFTER" || exit 1
    diff=$(compare -metric AE "$BEFORE" "$AFTER" null: 2>&1 || true)
    echo "  same process (pid $APP_PID), config hot-reloaded"
    echo "  before: $BEFORE"
    echo "  after:  $AFTER"
    echo "  changed pixels: $diff"
    OUT="$AFTER"
fi

echo "$OUT"
