#!/usr/bin/env bash
#
# Terminus development loop for the Windows desktop build.
#
# Cross-compiles on WSL (ext4) with cargo-xwin, copies only the final exe to
# NTFS, and signals a PowerShell watcher to kill/swap/restart the app.
#
#   scripts/dev-win.sh              watch, build, deploy, ensure watcher
#   scripts/dev-win.sh --once       build and deploy once
#   scripts/dev-win.sh --no-run     watch and build, do not start/signal watcher
#   scripts/dev-win.sh -- --flag    pass arguments through to rio-dev.exe
#
# Why a separate script: the Windows target needs cargo-xwin + clang-cl + the
# MSVC SDK. The cycle is slower than the Linux loop — use `scripts/dev.sh` for
# layout/colour work and this one to verify on real Windows GPU hardware.
#
# Interop is used only once to bootstrap `scripts/win-reload.ps1`. The save →
# rebuild → restart cycle goes through reload.stamp on NTFS and does not need
# cmd.exe. If interop is down (FHS sandbox replaces /init), paste the printed
# PowerShell command into a Windows terminal once.

set -euo pipefail

SCRIPT_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(cd "$(dirname "$SCRIPT_PATH")/.." && pwd)"

# ----------------------------------------------------------------- windows shell ---
# Enter nix develop .#windows once (cargo-xwin, clang-cl, rust-std-msvc). Do not
# pay for `nix shell` on every save.
if [[ "${TERMINUS_DEV_WIN_SHELL:-0}" != "1" && "${TERMINUS_DEV_NO_NIX:-0}" != "1" ]]; then
    if ! command -v nix >/dev/null 2>&1; then
        echo "dev-win.sh: nix not found and TERMINUS_DEV_NO_NIX is unset" >&2
        exit 1
    fi
    echo "dev-win.sh: entering nix develop .#windows (first run may fetch xwin SDK)"
    quoted_args=""
    if [ "$#" -gt 0 ]; then
        quoted_args="$(printf ' %q' "$@")"
    fi
    exec nix develop "$ROOT#windows" --command bash -c \
        "$(printf 'TERMINUS_DEV_WIN_SHELL=1 exec bash %q' "$SCRIPT_PATH")$quoted_args"
fi

TARGET="x86_64-pc-windows-msvc"
PACKAGE="rioterm"
ARTIFACT="$ROOT/target/$TARGET/debug/rio.exe"
# wgpu is forced on for Windows via target-specific deps; keep an override hook.
FEATURES="${TERMINUS_DEV_WIN_FEATURES:-}"
DEPLOY_DIR="${TERMINUS_DEV_WIN_DEPLOY_DIR:-}"
DEPLOY_NAME="rio-dev.exe"
WATCHER_SCRIPT="$ROOT/scripts/win-reload.ps1"
LOG_LEVEL="${RIO_LOG_LEVEL:-info}"
POLL_INTERVAL="${TERMINUS_DEV_POLL:-0.5}"
DEBOUNCE="${TERMINUS_DEV_DEBOUNCE:-0.4}"
WATCHER_ALIVE_MAX_AGE="${TERMINUS_DEV_WIN_WATCHER_MAX_AGE:-5}"

WATCH=1
RUN_APP=1
APP_ARGS=()

usage() {
    sed -n '3,22p' "$SCRIPT_PATH" | sed 's/^# \{0,1\}//'
    cat <<'EOF'

Options:
  --once              build and deploy once, do not watch
  --no-run            watch and build, do not bootstrap the Windows watcher
  --features <list>   extra cargo features
  --log-level <level> RIO_LOG_LEVEL for the app (default: info)
  -- <args...>        everything after -- is passed to the app (via watcher env is N/A;
                      args are written to app-args.txt for the watcher to pick up)
  -h, --help          this text

Environment:
  TERMINUS_DEV_WIN_FEATURES      same as --features
  TERMINUS_DEV_WIN_DEPLOY_DIR    NTFS deploy directory (default: %LOCALAPPDATA%\terminus-dev)
  RIO_LOG_LEVEL                  same as --log-level
  TERMINUS_DEV_POLL              seconds between source polls (default 0.5)
  TERMINUS_DEV_DEBOUNCE          seconds to wait for saves to settle (default 0.4)
  TERMINUS_DEV_NO_NIX=1          do not re-exec inside nix develop .#windows
  XWIN_CACHE_DIR                 cargo-xwin SDK cache (default: .dev/xwin-cache)
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --once) WATCH=0 ;;
        --no-run) RUN_APP=0 ;;
        --features) FEATURES="${2:?--features needs a value}"; shift ;;
        --log-level) LOG_LEVEL="${2:?--log-level needs a value}"; shift ;;
        --) shift; APP_ARGS=("$@"); break ;;
        -h|--help) usage; exit 0 ;;
        *) echo "dev-win.sh: unknown option '$1' (see --help)" >&2; exit 2 ;;
    esac
    shift
done

# --------------------------------------------------------------- deploy dir ---
# Prefer %LOCALAPPDATA%\terminus-dev via /mnt/c/Users/<user>/AppData/Local.
resolve_deploy_dir() {
    if [[ -n "$DEPLOY_DIR" ]]; then
        printf '%s\n' "$DEPLOY_DIR"
        return
    fi
    local candidate
    for candidate in /mnt/c/Users/*/AppData/Local; do
        [[ -d "$candidate" ]] || continue
        case "$candidate" in
            */Public/*|*/Default*|*/Default\ User*|*/All\ Users*) continue ;;
        esac
        printf '%s/terminus-dev\n' "$candidate"
        return
    done
    return 1
}

if ! DEPLOY_DIR="$(resolve_deploy_dir)"; then
    echo "dev-win.sh: no /mnt/c/Users/*/AppData/Local found; pass TERMINUS_DEV_WIN_DEPLOY_DIR" >&2
    exit 1
fi

DEV_DIR="$ROOT/.dev"
CONFIG_DIR="$DEV_DIR/config"
LOG_DIR="$DEV_DIR/logs"
BUILD_LOG="$LOG_DIR/build-win.log"
STAMP="$DEV_DIR/.watch-stamp-win"
DEBOUNCE_REF="$DEV_DIR/.watch-ref-win"
XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-$DEV_DIR/xwin-cache}"

DEPLOY_EXE="$DEPLOY_DIR/$DEPLOY_NAME"
DEPLOY_NEW="$DEPLOY_DIR/$DEPLOY_NAME.new"
RELOAD_STAMP="$DEPLOY_DIR/reload.stamp"
WATCHER_ALIVE="$DEPLOY_DIR/watcher.alive"
APP_ARGS_FILE="$DEPLOY_DIR/app-args.txt"

mkdir -p "$CONFIG_DIR" "$LOG_DIR" "$XWIN_CACHE_DIR" "$DEPLOY_DIR"
export XWIN_CACHE_DIR

# Baseline config for live colour/padding edits (same as scripts/dev.sh).
if [[ ! -f "$CONFIG_DIR/config.toml" && -f "$ROOT/dev/config/config.toml" ]]; then
    cp "$ROOT/dev/config/config.toml" "$CONFIG_DIR/config.toml"
fi

# /mnt/c/Users/x/... -> C:\Users\x\...
win_path() {
    local p="$1"
    if command -v wslpath >/dev/null 2>&1 && wslpath -w "$p" >/dev/null 2>&1; then
        wslpath -w "$p"
        return
    fi
    if [[ "$p" =~ ^/mnt/([a-zA-Z])/(.*)$ ]]; then
        printf '%s:\\%s\n' "${BASH_REMATCH[1]^^}" "${BASH_REMATCH[2]//\//\\}"
    else
        printf '%s\n' "$p"
    fi
}

# UNC path into this distro for RIO_CONFIG_HOME (live edit from WSL).
config_home_win() {
    local distro="${WSL_DISTRO_NAME:-}"
    if [[ -z "$distro" && -r /etc/wsl.conf ]]; then
        distro="$(awk -F= '/^\[network\]/{s=1} s&&/hostname/{gsub(/ /,"",$2); print $2; exit}' /etc/wsl.conf 2>/dev/null || true)"
    fi
    if [[ -z "$distro" ]]; then
        distro="$(cat /etc/hostname 2>/dev/null || echo NixOS)"
    fi
    # \\wsl$\Distro\home\nixos\... — backslashes required for some WinAPIs.
    local unc_tail="${CONFIG_DIR//'/'/'\\'}"
    printf '\\\\wsl$\\%s%s\n' "$distro" "$unc_tail"
}

CONFIG_HOME_WIN="$(config_home_win)"
DEPLOY_DIR_WIN="$(win_path "$DEPLOY_DIR")"

# Stage the watcher on NTFS so the user can double-click it even when
# WSL->Windows interop is dead (common inside nix develop / FHS).
# Keep the .cmd ASCII-only with CRLF: a UTF-8 em-dash in a REM line makes
# cmd.exe spit "'M' is not recognized" on French Windows code pages.
DEPLOY_WATCHER="$DEPLOY_DIR/win-reload.ps1"
DEPLOY_START_CMD="$DEPLOY_DIR/start-watcher.cmd"
cp -f "$WATCHER_SCRIPT" "$DEPLOY_WATCHER"
WATCHER_SCRIPT_WIN="$(win_path "$DEPLOY_WATCHER")"
# Write CRLF via printf so cmd.exe is happy when double-clicked.
{
    printf '@echo off\r\n'
    printf 'cd /d "%%~dp0"\r\n'
    printf 'powershell -NoProfile -ExecutionPolicy Bypass -File "%%~dp0win-reload.ps1" -DeployDir "%%~dp0." -ConfigHome "%s" -LogLevel "%s"\r\n' \
        "$CONFIG_HOME_WIN" "$LOG_LEVEL"
} >"$DEPLOY_START_CMD"
# Persist app args for the watcher (Start-Process ArgumentList).
if ((${#APP_ARGS[@]})); then
    printf '%s\n' "${APP_ARGS[@]}" >"$APP_ARGS_FILE"
else
    rm -f "$APP_ARGS_FILE"
fi

# ------------------------------------------------------------------- watch ---
WATCH_PATHS=()
for path in \
    "frontends/rioterm" \
    "rio-backend/src" \
    "rio-vt/src" \
    "rio-grid/src" \
    "sugarloaf/src" \
    "rio-window/src" \
    "crates"
do
    [[ -e "$ROOT/$path" ]] && WATCH_PATHS+=("$ROOT/$path")
done

changed_since() {
    find "${WATCH_PATHS[@]}" \
        \( -name '*.rs' -o -name '*.toml' -o -name '*.glsl' -o -name '*.wgsl' -o -name '*.slint' \) \
        -newer "$1" -print -quit 2>/dev/null
}

# ---------------------------------------------------------------- interop ----
find_cmd_exe() {
    local c
    for c in cmd.exe /mnt/c/Windows/System32/cmd.exe /mnt/c/WINDOWS/system32/cmd.exe; do
        [[ -x "$c" ]] || command -v "$c" >/dev/null 2>&1 || continue
        if "$c" /c echo ok >/dev/null 2>&1; then
            printf '%s\n' "$c"
            return 0
        fi
    done
    return 1
}

find_powershell_exe() {
    local c
    for c in powershell.exe /mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe \
        /mnt/c/WINDOWS/System32/WindowsPowerShell/v1.0/powershell.exe; do
        [[ -x "$c" ]] || command -v "$c" >/dev/null 2>&1 || continue
        # Prefer existence; may still fail under broken interop — caller checks.
        printf '%s\n' "$c"
        return 0
    done
    return 1
}

watcher_alive() {
    [[ -f "$WATCHER_ALIVE" ]] || return 1
    local age
    age=$(( $(date +%s) - $(stat -c %Y "$WATCHER_ALIVE" 2>/dev/null || echo 0) ))
    [[ "$age" -le "$WATCHER_ALIVE_MAX_AGE" ]]
}

print_watcher_cmd() {
    cat <<EOF
powershell -NoProfile -ExecutionPolicy Bypass -File "$WATCHER_SCRIPT_WIN" -DeployDir "$DEPLOY_DIR_WIN" -ConfigHome "$CONFIG_HOME_WIN" -LogLevel "$LOG_LEVEL"
EOF
}

print_watcher_fallback() {
    printf '\033[33m  WSL interop unavailable — do this once on Windows:\033[0m\n'
    printf '    1. Close any running rio-dev.exe\n'
    printf '    2. Double-click: %s\\start-watcher.cmd\n' "$DEPLOY_DIR_WIN"
    printf '       (or paste in PowerShell:)\n'
    print_watcher_cmd | sed 's/^/         /'
}

ensure_watcher() {
    [[ "$RUN_APP" == "1" ]] || return 0
    if watcher_alive; then
        printf '  watcher alive (%s)\n' "$WATCHER_ALIVE"
        return 0
    fi

    local ps_exe="" cmd_exe=""
    ps_exe="$(find_powershell_exe 2>/dev/null || true)"
    cmd_exe="$(find_cmd_exe 2>/dev/null || true)"

    if [[ -n "$cmd_exe" && -n "$ps_exe" ]]; then
        printf '  bootstrapping Windows watcher via interop…\n'
        # Start-Process so the watcher outlives this shell.
        if "$cmd_exe" /c "start \"\" \"$ps_exe\" -NoProfile -ExecutionPolicy Bypass -File \"$WATCHER_SCRIPT_WIN\" -DeployDir \"$DEPLOY_DIR_WIN\" -ConfigHome \"$CONFIG_HOME_WIN\" -LogLevel \"$LOG_LEVEL\"" \
            >/dev/null 2>&1
        then
            local i
            for i in $(seq 1 20); do
                sleep 0.25
                watcher_alive && {
                    printf '  watcher started\n'
                    return 0
                }
            done
            print_watcher_fallback
            return 0
        fi
    fi

    print_watcher_fallback
}

# ------------------------------------------------------------------- build ---
cargo_args=(-p "$PACKAGE" --target "$TARGET")
[[ -n "$FEATURES" ]] && cargo_args+=(--features "$FEATURES")

build() {
    local started=$SECONDS
    printf '\033[2m[%s] cross-building %s for %s%s…\033[0m\n' "$(date +%H:%M:%S)" "$PACKAGE" "$TARGET" \
        "${FEATURES:+ --features $FEATURES}"
    # Prefer `cargo xwin` when available (shell has cargo-xwin); fall back to
    # plain cargo if the user set TERMINUS_DEV_NO_NIX with a pre-provisioned SDK.
    local -a cmd=(cargo xwin build)
    if ! command -v cargo-xwin >/dev/null 2>&1 && ! cargo xwin --help >/dev/null 2>&1; then
        cmd=(cargo build)
    fi
    if "${cmd[@]}" "${cargo_args[@]}" >"$BUILD_LOG" 2>&1; then
        printf '\033[32m[%s] built in %ss\033[0m\n' "$(date +%H:%M:%S)" "$((SECONDS - started))"
        return 0
    fi
    printf '\033[31m[%s] build failed:\033[0m\n' "$(date +%H:%M:%S)"
    grep -E '^(error|warning: unused|  -->|help:|note:)' "$BUILD_LOG" | head -40 || true
    echo "   full output: $BUILD_LOG"
    return 1
}

deploy() {
    [[ -f "$ARTIFACT" ]] || { echo "dev-win.sh: $ARTIFACT missing" >&2; return 1; }
    # Never overwrite a running exe: write .new, then touch the stamp so the
    # PowerShell watcher does taskkill + Move-Item + Start-Process.
    if ! install -m 0755 "$ARTIFACT" "$DEPLOY_NEW" 2>/dev/null; then
        sleep 0.3
        install -m 0755 "$ARTIFACT" "$DEPLOY_NEW" || {
            printf '\033[33m[%s] deploy failed — is %s locked?\033[0m\n' \
                "$(date +%H:%M:%S)" "$DEPLOY_NAME"
            return 1
        }
    fi

    if watcher_alive; then
        touch "$RELOAD_STAMP"
        printf '\033[32m[%s] deployed %s (watcher will restart)\033[0m\n' \
            "$(date +%H:%M:%S)" "$DEPLOY_NEW"
    else
        # No watcher: try to replace the exe in place so a manual launch
        # picks up the new build. Fails if Windows still has it open.
        if cp -f "$DEPLOY_NEW" "$DEPLOY_EXE" 2>/dev/null; then
            touch "$RELOAD_STAMP"
            printf '\033[32m[%s] deployed %s (no watcher — relaunch the exe on Windows)\033[0m\n' \
                "$(date +%H:%M:%S)" "$DEPLOY_EXE"
            printf '  path: %s\n' "$(win_path "$DEPLOY_EXE")"
            printf '  tip:  double-click %s\\start-watcher.cmd for auto-restart\n' "$DEPLOY_DIR_WIN"
        else
            touch "$RELOAD_STAMP"
            printf '\033[33m[%s] wrote %s but could not replace %s (still running?)\033[0m\n' \
                "$(date +%H:%M:%S)" "$DEPLOY_NEW" "$DEPLOY_NAME"
            printf '  close rio-dev.exe on Windows, then either:\n'
            printf '    - double-click %s\\start-watcher.cmd\n' "$DEPLOY_DIR_WIN"
            printf '    - or copy rio-dev.exe.new → rio-dev.exe and relaunch\n'
        fi
    fi
}

# -------------------------------------------------------------------- main ---
printf '\033[1mTerminus Windows dev loop\033[0m\n'
printf '  target:  %s\n' "$TARGET"
printf '  deploy:  %s\n' "$DEPLOY_DIR"
printf '  config:  %s\n' "$CONFIG_HOME_WIN"
printf '  logs:    %s  (build), %s\\run-win.log  (app)\n' "$BUILD_LOG" "$DEPLOY_DIR_WIN"

ensure_watcher

touch "$STAMP"
if build; then
    deploy || true
else
    :
fi

if [[ "$WATCH" == "0" ]]; then
    exit 0
fi

printf '  watching %s paths, Ctrl-C to stop\n' "${#WATCH_PATHS[@]}"

while :; do
    sleep "$POLL_INTERVAL"
    hit="$(changed_since "$STAMP")"
    [[ -z "$hit" ]] && continue

    touch -r "$hit" "$DEBOUNCE_REF" 2>/dev/null || touch "$DEBOUNCE_REF"
    while :; do
        sleep "$DEBOUNCE"
        hit="$(changed_since "$DEBOUNCE_REF")"
        [[ -z "$hit" ]] && break
        touch -r "$hit" "$DEBOUNCE_REF" 2>/dev/null || touch "$DEBOUNCE_REF"
    done

    touch "$STAMP"
    if build; then
        deploy || true
        # Re-check watcher periodically in case it was killed.
        if [[ "$RUN_APP" == "1" ]] && ! watcher_alive; then
            ensure_watcher
        fi
    fi
done
