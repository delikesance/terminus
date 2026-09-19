#!/usr/bin/env bash
#
# Terminus release build & publish script (Linux & Windows).
#
# Usage:
#   nix run .#release               # build Linux + Windows and deploy to GitHub
#   nix run .#release -- --build-only # build Linux + Windows without deploying
#   nix run .#release -- --tag v0.5.29 # build and deploy for tag v0.5.29

set -euo pipefail
SCRIPT_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(cd "$(dirname "$SCRIPT_PATH")/.." && pwd)"
cd "$ROOT"

if [[ "${TERMINUS_RELEASE_SHELL:-0}" != "1" && "${TERMINUS_DEV_NO_NIX:-0}" != "1" ]]; then
    if ! command -v nix >/dev/null 2>&1; then
        echo "release.sh: nix not found and TERMINUS_DEV_NO_NIX is unset" >&2
        exit 1
    fi
    echo "release.sh: entering nix shell... (will download required dependencies if missing)"
    exec nix run "$ROOT#release" -- "$@"
fi

VERSION="$(grep -m 1 '^version = ' Cargo.toml | awk -F '"' '{print $2}')"
TAG="v${VERSION}"
TITLE="Terminus ${TAG}"
BUILD_ONLY=0
LINUX_ONLY=0
WINDOWS_ONLY=0
GH_FLAGS=()
EXTRA_GH_FLAGS=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --build-only|--no-deploy) BUILD_ONLY=1 ;;
        --deploy) BUILD_ONLY=0 ;;
        --linux-only) LINUX_ONLY=1 ;;
        --windows-only) WINDOWS_ONLY=1 ;;
        --tag) TAG="$2"; shift ;;
        --draft) EXTRA_GH_FLAGS="$EXTRA_GH_FLAGS --draft" ;;
        --prerelease) EXTRA_GH_FLAGS="$EXTRA_GH_FLAGS --prerelease" ;;
        --title) TITLE="$2"; shift ;;
        --notes) EXTRA_GH_FLAGS="$EXTRA_GH_FLAGS --notes '$2'"; shift ;;
        -h|--help)
            echo "Usage: nix run .#release [options]"
            echo "Options:"
            echo "  --build-only     Do not upload to GitHub releases"
            echo "  --linux-only     Only build Linux artifacts"
            echo "  --windows-only   Only cross-compile Windows artifacts"
            echo "  --tag <tag>      Target release tag (default: $TAG)"
            echo "  --draft          Mark GitHub release as draft"
            echo "  --prerelease     Mark GitHub release as pre-release"
            echo "  --title <title>  GitHub release title"
            echo "  --notes <notes>  GitHub release body"
            exit 0
            ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
    shift
done

export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-false}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-16}"
export CARGO_PROFILE_RELEASE_DEBUG="${CARGO_PROFILE_RELEASE_DEBUG:-0}"

DIST_DIR="$ROOT/dist"
mkdir -p "$DIST_DIR"

if [[ "$WINDOWS_ONLY" == "0" ]]; then
    echo "=== Building Linux release (rioterm with wgpu) ==="
    cargo build --release -p rioterm --features wgpu

    echo "Packaging Linux artifacts..."
    BIN="$(find target/release -maxdepth 1 -type f -name rio -executable | head -1)"
    if [[ -z "$BIN" ]]; then
        echo "Error: Could not find built rio binary in target/release" >&2
        exit 1
    fi
    cp "$BIN" "$DIST_DIR/rio-linux-x86_64"
    chmod +x "$DIST_DIR/rio-linux-x86_64"

    TAR_DIR="$(mktemp -d)"
    mkdir -p "$TAR_DIR/rio"
    cp "$BIN" "$TAR_DIR/rio/rio"
    cp misc/rio.desktop "$TAR_DIR/rio/" 2>/dev/null || true
    cp misc/logo.svg "$TAR_DIR/rio/" 2>/dev/null || true
    cp misc/rio.terminfo "$TAR_DIR/rio/" 2>/dev/null || true
    tar -czf "$DIST_DIR/rio-linux-x86_64.tar.gz" -C "$TAR_DIR" rio
    rm -rf "$TAR_DIR"
fi

if [[ "$LINUX_ONLY" == "0" ]]; then
    echo "=== Building Windows release (x86_64-pc-windows-msvc with wgpu) ==="
    export XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-$ROOT/.dev/xwin-cache}"
    mkdir -p "$XWIN_CACHE_DIR"
    
    cargo xwin build --release -p rioterm --target x86_64-pc-windows-msvc --features wgpu

    echo "Packaging Windows artifacts..."
    WIN_BIN="$(find target/x86_64-pc-windows-msvc/release -maxdepth 1 -type f -name rio.exe | head -1)"
    if [[ -z "$WIN_BIN" ]]; then
        echo "Error: Could not find built rio.exe in target/x86_64-pc-windows-msvc/release" >&2
        exit 1
    fi
    cp "$WIN_BIN" "$DIST_DIR/rio.exe"
    
    (cd "$DIST_DIR" && rm -f rio-windows-x86_64.zip && 7z a -tzip rio-windows-x86_64.zip rio.exe >/dev/null || zip -q rio-windows-x86_64.zip rio.exe)
fi

echo "=== Generating checksums ==="
(cd "$DIST_DIR" && sha256sum rio* > checksums.txt)

if [[ "$BUILD_ONLY" == "1" ]]; then
    echo "Build complete. Artifacts are in $DIST_DIR."
    ls -lah "$DIST_DIR"
    exit 0
fi

echo "=== Deploying to GitHub Release ($TAG) ==="
if ! command -v gh >/dev/null 2>&1; then
    echo "Error: 'gh' cli not found or not authenticated." >&2
    exit 1
fi

if gh release view "$TAG" >/dev/null 2>&1; then
    echo "Release $TAG already exists. Uploading missing or updated artifacts..."
    gh release upload "$TAG" "$DIST_DIR"/* --clobber
else
    echo "Creating new release $TAG..."
    # Intentionally expanded EXTRA_GH_FLAGS
    # shellcheck disable=SC2086
    gh release create "$TAG" "$DIST_DIR"/* --title "$TITLE" --generate-notes $EXTRA_GH_FLAGS
fi

echo "Deployment complete."
