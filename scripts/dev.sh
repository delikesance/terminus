#!/usr/bin/env bash
#
# Terminus development loop with hot reload.
#
#   scripts/dev.sh                 watch sources, rebuild, restart the app
#   scripts/dev.sh --once          build and run once, then exit
#   scripts/dev.sh --no-run        watch and rebuild, never launch the app
#   scripts/dev.sh -- --some-arg   pass arguments through to `rio`
#
# What is actually "hot" here:
#
#   * Rust code — no. Rust has no sound way to swap a running binary's code, so
#     a save triggers an incremental rebuild and a process restart. The loop is
#     tuned so that restart is a couple of seconds, not a coffee break:
#     dependencies are built once at `opt-level = 2` (see [profile.dev] in
#     Cargo.toml) while workspace crates stay unoptimized, and only the crates
#     you touched are recompiled.
#   * Configuration and colours — yes, genuinely live. The app watches its
#     config directory, so saving `.dev/config/config.toml` re-paints the
#     running window with no restart at all. That is the loop to use while
#     tuning margins, paddings, control heights, palettes and fonts.
#
# The script re-execs itself inside the project's nix devshell when it is not
# already running in one. That is not an optimisation: on this machine a plain
# shell exports PKG_CONFIG_PATH=/usr/lib/pkgconfig, which has no fontconfig.pc,
# and `yeslogic-fontconfig-sys` then panics in its build script, so `cargo
# build -p rioterm` cannot succeed outside the devshell.

set -euo pipefail

SCRIPT_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(cd "$(dirname "$SCRIPT_PATH")/.." && pwd)"

# ---------------------------------------------------------------- devshell ---
# `TERMINUS_DEV_NO_NIX=1` skips the devshell re-exec (for shells that already
# have the right PKG_CONFIG_PATH / LD_LIBRARY_PATH).
if [[ "${TERMINUS_DEV_SHELL:-0}" != "1" && "${TERMINUS_DEV_NO_NIX:-0}" != "1" ]]; then
    if ! command -v nix >/dev/null 2>&1; then
        echo "dev.sh: nix not found and TERMINUS_DEV_NO_NIX is unset" >&2
        echo "dev.sh: retry with TERMINUS_DEV_NO_NIX=1 in an environment that has" >&2
        echo "dev.sh: fontconfig's pkg-config file on PKG_CONFIG_PATH" >&2
        exit 1
    fi
    echo "dev.sh: entering nix devshell (first run may evaluate nixpkgs, ~15s)"
    # Re-exec inside the flake devshell: it is what puts fontconfig's pkg-config
    # file on PKG_CONFIG_PATH (the build scripts need it) and the X11/Wayland/
    # Vulkan runtime libraries on LD_LIBRARY_PATH.
    quoted_args=""
    if [ "$#" -gt 0 ]; then
        quoted_args="$(printf ' %q' "$@")"
    fi
    exec nix develop "$ROOT" --command bash -c \
        "$(printf 'TERMINUS_DEV_SHELL=1 exec bash %q' "$SCRIPT_PATH")$quoted_args"
fi

PACKAGE="rioterm"
BUILD_LOG=""
RUN_LOG=""
WATCH=1
RUN_APP=1
FEATURES="${TERMINUS_DEV_FEATURES:-}"
LOG_LEVEL="${RIO_LOG_LEVEL:-info}"
POLL_INTERVAL="${TERMINUS_DEV_POLL:-0.5}"
DEBOUNCE="${TERMINUS_DEV_DEBOUNCE:-0.4}"
APP_ARGS=()

usage() {
    sed -n '3,22p' "$SCRIPT_PATH" | sed 's/^# \{0,1\}//'
    cat <<'EOF'

Options:
  --once              build and launch once, do not watch
  --no-run            watch and rebuild, do not launch the app
  --features <list>   extra cargo features (e.g. wgpu)
  --log-level <level> RIO_LOG_LEVEL for the app (default: info)
  -- <args...>        everything after -- is passed to the app
  -h, --help          this text

Environment:
  TERMINUS_DEV_FEATURES   same as --features
  RIO_LOG_LEVEL           same as --log-level
  TERMINUS_DEV_POLL       seconds between source polls (default 0.5)
  TERMINUS_DEV_DEBOUNCE   seconds to wait for saves to settle (default 0.4)
  TERMINUS_DEV_NO_NIX=1   do not re-exec inside the nix devshell
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
        *) echo "dev.sh: unknown option '$1' (see --help)" >&2; exit 2 ;;
    esac
    shift
done

DEV_DIR="$ROOT/.dev"
CONFIG_DIR="$DEV_DIR/config"
LOG_DIR="$DEV_DIR/logs"
BUILD_LOG="$LOG_DIR/build.log"
RUN_LOG="$LOG_DIR/run.log"
PID_FILE="$DEV_DIR/app.pid"
STAMP="$DEV_DIR/.watch-stamp"
DEBOUNCE_REF="$DEV_DIR/.watch-ref"
BIN="$ROOT/target/debug/rio"

mkdir -p "$CONFIG_DIR" "$LOG_DIR"

# ------------------------------------------------------------- dev config ----
# Baseline config, copied in once so the app has something valid to hot-reload
# and so a stray write from the app never lands in the tracked tree. Delete
# .dev/config to start over from the baseline.
if [[ ! -f "$CONFIG_DIR/config.toml" && -f "$ROOT/dev/config/config.toml" ]]; then
    cp "$ROOT/dev/config/config.toml" "$CONFIG_DIR/config.toml"
fi

# --------------------------------------------------------- software vulkan ---
# WSLg exposes no Vulkan ICD of its own on this machine, so the native renderer
# finds no device. Append mesa's lavapipe (CPU) ICD when nothing else is
# installed. Set TERMINUS_DEV_VULKAN_ICD=0 to skip.
if [[ "${TERMINUS_DEV_VULKAN_ICD:-1}" == "1" && -z "${VK_ICD_FILENAMES:-}" ]]; then
    if ! compgen -G "/usr/share/vulkan/icd.d/*.json" >/dev/null 2>&1; then
        gpu_env="$DEV_DIR/gpu-env.sh"
        if [[ ! -s "$gpu_env" ]]; then
            mesa_path="$(nix build --no-link --print-out-paths nixpkgs#mesa 2>/dev/null | tail -1 || true)"
            mesa_icd=""
            if [[ -n "$mesa_path" ]]; then
                mesa_icd="$mesa_path/share/vulkan/icd.d/lvp_icd.x86_64.json"
                [[ -f "$mesa_icd" ]] || mesa_icd="$(ls "$mesa_path"/share/vulkan/icd.d/*.json 2>/dev/null | head -1)"
            fi
            if [[ -n "$mesa_icd" && -f "$mesa_icd" ]]; then
                {
                    echo "export LD_LIBRARY_PATH=\"$mesa_path/lib:\${LD_LIBRARY_PATH:-}\""
                    echo "export VK_ICD_FILENAMES=\"$mesa_icd\""
                } >"$gpu_env"
            else
                echo "dev.sh: could not resolve a mesa Vulkan ICD, running without one" >&2
                : >"$gpu_env"
            fi
        fi
        # shellcheck disable=SC1090
        [[ -s "$gpu_env" ]] && . "$gpu_env"
    fi
fi

# ------------------------------------------------------------------ watch ----
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

# Anything newer than $1 means "rebuild me".
changed_since() {
    find "${WATCH_PATHS[@]}" \
        \( -name '*.rs' -o -name '*.toml' -o -name '*.glsl' -o -name '*.wgsl' -o -name '*.slint' \) \
        -newer "$1" -print -quit 2>/dev/null
}

# ---------------------------------------------------------------- process ----
app_pid() {
    [[ -s "$PID_FILE" ]] || return 1
    local pid
    pid="$(cat "$PID_FILE")"
    kill -0 "$pid" 2>/dev/null || return 1
    printf '%s' "$pid"
}

stop_app() {
    local pid
    if pid="$(app_pid)"; then
        kill "$pid" 2>/dev/null || true
        for _ in $(seq 1 30); do
            kill -0 "$pid" 2>/dev/null || break
            sleep 0.1
        done
        kill -0 "$pid" 2>/dev/null && kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$PID_FILE"
}

# `nix develop` puts nixpkgs' build-time bash first in PATH and points SHELL at
# it. That bash is compiled without readline, so a shell spawned by the dev loop
# answers every key *except* Tab, Up and Ctrl-R: the byte reaches the PTY, the
# tty echoes it, and nothing completes, nothing recalls, because readline is not
# there to interpret `\t` as complete. Editing, colours and Ctrl-C all keep
# working, which makes it look like a terminal bug instead of a PATH accident.
# Hand the spawned shell a bash that actually carries the `bind` builtin.
find_readline_shell() {
    local candidate
    for candidate in "${TERMINUS_SHELL:-}" "$(command -v bashInteractive || true)" \
        /run/current-system/sw/bin/bash /usr/bin/bash /bin/bash; do
        [[ -n "$candidate" && -x "$candidate" ]] || continue
        if "$candidate" -c 'type -t bind' >/dev/null 2>&1; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

start_app() {
    if [[ ! -x "$BIN" ]]; then
        echo "dev.sh: $BIN is missing, skipping launch" >&2
        return 1
    fi
    local -a shell_env=()
    local shell_bin
    if shell_bin="$(find_readline_shell)"; then
        shell_env=(SHELL="$shell_bin")
    fi
    : >"$RUN_LOG"
    (
        cd "$ROOT"
        exec env "${shell_env[@]}" \
            RIO_CONFIG_HOME="$CONFIG_DIR" \
            RIO_LOG_LEVEL="$LOG_LEVEL" \
            "$BIN" "${APP_ARGS[@]}"
    ) >>"$RUN_LOG" 2>&1 &
    echo $! >"$PID_FILE"
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM
    stop_app
    exit "$status"
}
trap cleanup EXIT INT TERM

# ------------------------------------------------------------------ build ----
cargo_args=(-p "$PACKAGE")
[[ -n "$FEATURES" ]] && cargo_args+=(--features "$FEATURES")

build() {
    local started=$SECONDS
    printf '\033[2m[%s] building %s%s…\033[0m\n' "$(date +%H:%M:%S)" "$PACKAGE" \
        "${FEATURES:+ --features $FEATURES}"
    if cargo build "${cargo_args[@]}" >"$BUILD_LOG" 2>&1; then
        printf '\033[32m[%s] built in %ss\033[0m\n' "$(date +%H:%M:%S)" "$((SECONDS - started))"
        return 0
    fi
    printf '\033[31m[%s] build failed:\033[0m\n' "$(date +%H:%M:%S)"
    # Show the diagnostics, not the "Compiling …" noise.
    grep -E '^(error|warning: unused|  -->|help:|note:)' "$BUILD_LOG" | head -40 || true
    echo "   full output: $BUILD_LOG"
    return 1
}

# ------------------------------------------------------------------- run -----
printf '\033[1mTerminus dev loop\033[0m  config: %s\n' "$CONFIG_DIR"
printf '  logs:   %s  (app), %s  (build)\n' "$RUN_LOG" "$BUILD_LOG"
if [[ "$RUN_APP" == "1" ]]; then
    printf '  edit %s to see colours/padding change without a restart\n' "$CONFIG_DIR/config.toml"
fi

touch "$STAMP"
build || true

if [[ "$RUN_APP" == "1" ]]; then
    start_app && printf '  app pid %s\n' "$(cat "$PID_FILE")"
fi

if [[ "$WATCH" == "0" ]]; then
    if [[ "$RUN_APP" == "1" ]]; then
        echo "  running once; Ctrl-C to stop"
        wait "$(cat "$PID_FILE")" 2>/dev/null || true
    fi
    exit 0
fi

printf '  watching %s paths, Ctrl-C to stop\n' "${#WATCH_PATHS[@]}"

while :; do
    sleep "$POLL_INTERVAL"
    hit="$(changed_since "$STAMP")"
    [[ -z "$hit" ]] && continue

    # Let a multi-file save settle: keep moving the reference forward until no
    # file is newer than the last one we saw.
    ref="$DEBOUNCE_REF"
    touch -r "$hit" "$ref" 2>/dev/null || touch "$ref"
    while :; do
        sleep "$DEBOUNCE"
        hit="$(changed_since "$ref")"
        [[ -z "$hit" ]] && break
        touch -r "$hit" "$ref" 2>/dev/null || touch "$ref"
    done

    # Stamp before building so edits made *during* the build are not missed.
    touch "$STAMP"
    if build; then
        if [[ "$RUN_APP" == "1" ]]; then
            stop_app
            start_app && printf '\033[32m[%s] restarted (pid %s)\033[0m\n' \
                "$(date +%H:%M:%S)" "$(cat "$PID_FILE")"
        fi
    else
        printf '  keeping the previous process alive\n'
    fi
done
