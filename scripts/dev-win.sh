#!/usr/bin/env bash
#
# Terminus development loop for the Windows build (the one you actually run on
# the desktop).
#
#   scripts/dev-win.sh              watch sources, cross-build, deploy the exe
#   scripts/dev-win.sh --once       build and deploy once
#   scripts/dev-win.sh --no-deploy  watch and build, do not copy the exe
#
# Why a separate script: the Windows target is cross-compiled with cargo-xwin,
# which needs clang/llvm/nasm and the MSVC SDK. The cycle is inherently slower
# than the Linux loop — a link of rio.exe dominates — so use
# `scripts/dev.sh` for layout/colour work and this one to verify on real
# hardware.
#
# Note: this machine's WSL has Windows interop registered but non-functional
# (`powershell.exe` fails with "cannot execute binary file"), so the script
# cannot restart the app for you. It deploys the exe and tells you where it is.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="x86_64-pc-windows-msvc"
PACKAGE="rioterm"
ARTIFACT="$ROOT/target/$TARGET/debug/rio.exe"
FEATURES="${TERMINUS_DEV_WIN_FEATURES:-wgpu}"
DEPLOY_DIR="${TERMINUS_DEV_WIN_DEPLOY_DIR:-}"
DEPLOY_NAME="rio-dev.exe"

WATCH=1
DEPLOY=1

while [[ $# -gt 0 ]]; do
    case "$1" in
        --once) WATCH=0 ;;
        --no-deploy) DEPLOY=0 ;;
        --features) FEATURES="${2:?--features needs a value}"; shift ;;
        -h|--help) sed -n '3,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "dev-win.sh: unknown option '$1'" >&2; exit 2 ;;
    esac
    shift
done

# Pick the Windows user's Desktop unless told otherwise.
if [[ -z "$DEPLOY_DIR" ]]; then
    DEPLOY_DIR="$(ls -d /mnt/c/Users/*/Desktop 2>/dev/null | grep -v -e '/Public/' -e '/Default' | head -1 || true)"
fi
if [[ -z "$DEPLOY_DIR" ]]; then
    echo "dev-win.sh: no /mnt/c/Users/*/Desktop found; pass TERMINUS_DEV_WIN_DEPLOY_DIR" >&2
    DEPLOY=0
fi

LOG_DIR="$ROOT/.dev/logs"
BUILD_LOG="$LOG_DIR/build-win.log"
STAMP="$ROOT/.dev/.watch-stamp-win"
mkdir -p "$LOG_DIR"
mkdir -p "$ROOT/.dev"

WATCH_PATHS=()
for path in "frontends/rioterm" "rio-backend/src" "rio-vt/src" "rio-grid/src" "sugarloaf/src" "rio-window/src" "crates"; do
    [[ -e "$ROOT/$path" ]] && WATCH_PATHS+=("$ROOT/$path")
done

DEBOUNCE_REF="$ROOT/.dev/.watch-ref-win"

changed_since() {
    find "${WATCH_PATHS[@]}" \
        \( -name '*.rs' -o -name '*.toml' -o -name '*.glsl' -o -name '*.wgsl' \) \
        -newer "$1" -print -quit 2>/dev/null
}

# /mnt/c/Users/x/Desktop/rio-dev.exe -> C:\Users\x\Desktop\rio-dev.exe
# (wslpath needs interop, which is broken in this WSL install).
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

build() {
    local started=$SECONDS
    printf '\033[2m[%s] cross-building %s for %s…\033[0m\n' "$(date +%H:%M:%S)" "$PACKAGE" "$TARGET"
    # clang-cl lives in clang-unwrapped, not in the `clang` wrapper package that
    # cargo-xwin's docs assume; without it cc-rs cannot build spirv-cross's C++.
    if nix shell nixpkgs#cargo-xwin nixpkgs#clang nixpkgs#llvmPackages.clang-unwrapped \
        nixpkgs#llvmPackages.bintools nixpkgs#nasm -c \
        cargo xwin build -p "$PACKAGE" --target "$TARGET" --features "$FEATURES" \
        >"$BUILD_LOG" 2>&1
    then
        printf '\033[32m[%s] built in %ss\033[0m\n' "$(date +%H:%M:%S)" "$((SECONDS - started))"
        return 0
    fi
    printf '\033[31m[%s] build failed:\033[0m\n' "$(date +%H:%M:%S)"
    grep -E '^(error|  -->)' "$BUILD_LOG" | head -30 || true
    echo "   full output: $BUILD_LOG"
    return 1
}

deploy() {
    [[ "$DEPLOY" == "1" ]] || return 0
    [[ -f "$ARTIFACT" ]] || { echo "dev-win.sh: $ARTIFACT missing" >&2; return 1; }
    if install -m 0755 "$ARTIFACT" "$DEPLOY_DIR/$DEPLOY_NAME" 2>/dev/null; then
        printf '\033[32m[%s] deployed %s\033[0m\n' "$(date +%H:%M:%S)" "$DEPLOY_DIR/$DEPLOY_NAME"
        printf '  launch it from Windows: %s\n' "$(win_path "$DEPLOY_DIR/$DEPLOY_NAME")"
    else
        printf '\033[33m[%s] deploy failed — is %s still running on Windows?\033[0m\n' \
            "$(date +%H:%M:%S)" "$DEPLOY_NAME"
    fi
}

touch "$STAMP"
build || true
deploy || true

if [[ "$WATCH" == "0" ]]; then
    exit 0
fi

printf '  watching %s paths, Ctrl-C to stop\n' "${#WATCH_PATHS[@]}"

while :; do
    sleep "${TERMINUS_DEV_POLL:-0.5}"
    hit="$(changed_since "$STAMP")"
    [[ -z "$hit" ]] && continue

    # Wait for multi-file saves to settle (see scripts/dev.sh for the same loop).
    touch -r "$hit" "$DEBOUNCE_REF" 2>/dev/null || touch "$DEBOUNCE_REF"
    while :; do
        sleep "${TERMINUS_DEV_DEBOUNCE:-0.4}"
        hit="$(changed_since "$DEBOUNCE_REF")"
        [[ -z "$hit" ]] && break
        touch -r "$hit" "$DEBOUNCE_REF" 2>/dev/null || touch "$DEBOUNCE_REF"
    done

    touch "$STAMP"
    build && deploy || true
done
