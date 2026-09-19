#!/usr/bin/env bash
#
# Terminus release build & publish (Linux x86_64 + Windows x86_64).
#
# Intended to run inside `nix develop .#release` (which provides cargo,
# cargo-xwin, the Windows MSVC std, fontconfig, krb5, gh, 7z and tar).
# The convenient entry point is the flake app:
#
#   nix run .#release                 # build Linux + Windows and publish
#   nix run .#release -- --build-only # build only, do not publish
#   nix run .#release -- --tag v1.2.3 # publish under an explicit tag

set -euo pipefail

SCRIPT_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
ROOT="$(cd "$(dirname "$SCRIPT_PATH")/.." && pwd)"
cd "$ROOT"

# Target repository for `gh`. Defaults to the `github` git remote so the
# release never lands on the upstream fork by accident.
REPO="${GH_REPO:-}"
if [[ -z "$REPO" ]]; then
    REPO="$(git remote get-url github 2>/dev/null | sed -E -e 's#\.git$##' -e 's#^[^:]+[:/]([^/]+/[^/]+)$#\1#')"
fi
[[ -z "$REPO" ]] && REPO="delikesance/terminus"
GITHUB_REMOTE_URL="${GITHUB_REMOTE_URL:-$(git remote get-url github 2>/dev/null || git remote get-url origin 2>/dev/null)}"

VERSION="$(grep -m 1 '^version = ' Cargo.toml | awk -F '"' '{print $2}')"
TAG="v${VERSION}"
TITLE="Terminus ${TAG}"
BUILD_ONLY=0
LINUX_ONLY=0
WINDOWS_ONLY=0
DRAFT=0
PRERELEASE=0
NOTES=""

usage() {
    sed -n '3,9p' "$SCRIPT_PATH" | sed 's/^# \{0,1\}//'
    cat <<'EOF'
Options:
  --build-only     Build artifacts but do not publish to GitHub
  --linux-only     Only build the Linux artifact
  --windows-only   Only cross-compile the Windows artifact
  --tag <tag>      Release tag (default: $TAG)
  --draft          Mark the GitHub release as draft
  --prerelease     Mark the GitHub release as pre-release
  --title <title>  Release title (default: Terminus $TAG)
  --notes <text>   Release body; omit to auto-generate notes
  -h, --help       Show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --build-only|--no-deploy) BUILD_ONLY=1 ;;
        --linux-only) LINUX_ONLY=1 ;;
        --windows-only) WINDOWS_ONLY=1 ;;
        --tag) TAG="${2:?--tag needs a value}"; shift ;;
        --title) TITLE="${2:?--title needs a value}"; shift ;;
        --notes) NOTES="${2:?--notes needs a value}"; shift ;;
        --draft) DRAFT=1 ;;
        --prerelease) PRERELEASE=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "release.sh: unknown option '$1' (see --help)" >&2; exit 2 ;;
    esac
    shift
done

# Fat LTO (Cargo.toml [profile.release]) can sit in the final link for an
# hour on constrained machines. Releases do not need it here; these env
# overrides are the same knobs the removed GitHub Actions release flow used.
export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-false}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-16}"
export CARGO_PROFILE_RELEASE_DEBUG="${CARGO_PROFILE_RELEASE_DEBUG:-0}"

DIST_DIR="$ROOT/dist"
mkdir -p "$DIST_DIR"
UPLOAD=()

if [[ "$WINDOWS_ONLY" == "0" ]]; then
    echo "=== Linux x86_64: cargo build --release -p rioterm --features wgpu ==="
    cargo build --release -p rioterm --features wgpu

    BIN="$(find target/release -maxdepth 1 -type f -name rio -executable | head -1)"
    if [[ -z "$BIN" ]]; then
        echo "release.sh: rio binary not found in target/release" >&2
        exit 1
    fi

    STAGE="$(mktemp -d)"
    mkdir -p "$STAGE/rio"
    cp "$BIN" "$STAGE/rio/rio"
    cp misc/rio.desktop "$STAGE/rio/" 2>/dev/null || true
    cp misc/logo.svg "$STAGE/rio/" 2>/dev/null || true
    cp misc/rio.terminfo "$STAGE/rio/" 2>/dev/null || true
    tar -czf "$DIST_DIR/rio-linux-x86_64.tar.gz" -C "$STAGE" rio
    rm -rf "$STAGE"

    cp "$BIN" "$DIST_DIR/rio-linux-x86_64"
    chmod +x "$DIST_DIR/rio-linux-x86_64"
    UPLOAD+=("$DIST_DIR/rio-linux-x86_64.tar.gz")
    echo "Linux artifacts written."
fi

if [[ "$LINUX_ONLY" == "0" ]]; then
    echo "=== Windows x86_64: cargo xwin build --release -p rioterm --target x86_64-pc-windows-msvc --features wgpu ==="
    export XWIN_CACHE_DIR="${XWIN_CACHE_DIR:-$ROOT/.dev/xwin-cache}"
    mkdir -p "$XWIN_CACHE_DIR"
    cargo xwin build --release -p rioterm --target x86_64-pc-windows-msvc --features wgpu

    WIN_BIN="$(find target/x86_64-pc-windows-msvc/release -maxdepth 1 -type f -name rio.exe | head -1)"
    if [[ -z "$WIN_BIN" ]]; then
        echo "release.sh: rio.exe not found in target/x86_64-pc-windows-msvc/release" >&2
        exit 1
    fi
    cp "$WIN_BIN" "$DIST_DIR/rio.exe"

    (cd "$DIST_DIR" && rm -f rio-windows-x86_64.zip && 7z a -tzip rio-windows-x86_64.zip rio.exe >/dev/null)
    UPLOAD+=("$DIST_DIR/rio-windows-x86_64.zip")
    echo "Windows artifacts written."
fi

echo "=== Checksums ==="
(cd "$DIST_DIR" && sha256sum "${UPLOAD[@]##*/}" > checksums.txt)
UPLOAD+=("$DIST_DIR/checksums.txt")

if [[ "$BUILD_ONLY" == "1" ]]; then
    echo "Build complete (no publish). Artifacts:"
    ls -lah "${UPLOAD[@]}"
    exit 0
fi

echo "=== GitHub release: $TAG (repo $REPO) ==="
if ! command -v gh >/dev/null 2>&1; then
    echo "release.sh: 'gh' is required to publish (gh not found in PATH)" >&2
    exit 1
fi

gh_flags=(--repo "$REPO")
[[ "$DRAFT" == "1" ]] && gh_flags+=(--draft)
[[ "$PRERELEASE" == "1" ]] && gh_flags+=(--prerelease)

if gh release view "$TAG" "${gh_flags[@]}" >/dev/null 2>&1; then
    echo "Release $TAG exists; uploading assets (clobber)."
    gh release upload "$TAG" "${UPLOAD[@]}" "${gh_flags[@]}" --clobber
else
    # `gh release create` cannot reuse an unpushed local tag, so push the
    # tag first when it exists (the prepare-release.sh ceremony tags before
    # releasing). When there is no tag yet, let gh create it at HEAD.
    if git tag -l "$TAG" >/dev/null; then
        echo "Pushing tag $TAG to $REPO..."
        git push "${GITHUB_REMOTE_URL}" "$TAG" || {
            echo "release.sh: failed to push tag $TAG" >&2
            exit 1
        }
    else
        gh_flags+=(--target "$(git rev-parse HEAD)")
    fi

    echo "Creating release $TAG."
    if [[ -n "$NOTES" ]]; then
        gh release create "$TAG" "${UPLOAD[@]}" --title "$TITLE" --notes "$NOTES" "${gh_flags[@]}"
    else
        gh release create "$TAG" "${UPLOAD[@]}" --title "$TITLE" --generate-notes "${gh_flags[@]}"
    fi
fi

echo "Done. Release: https://github.com/$REPO/releases/tag/$TAG"
