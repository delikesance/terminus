#!/usr/bin/env bash
#
# Terminus one-command release utility:
# Pulls latest changes from main branch, builds cross-platform artifacts
# (Linux + Windows), and publishes the release on GitHub.
#
# Usage:
#   ./scripts/publish.sh [options]
#   nix run .#publish -- [options]
#   make publish [ARGS="..."]
#
# Options:
#   --branch <branch>     Branch to release from (default: main)
#   --remote <remote>     Git remote to pull from (default: github or origin)
#   --build-only          Build and package artifacts locally without publishing
#   --tag <tag>           Explicit release tag (default: v<Cargo.toml version>)
#   --title <title>       Release title (default: Terminus <tag>)
#   --notes <text>        Release description / notes
#   --draft               Publish as a draft release
#   --prerelease          Publish as a pre-release
#   --skip-pull           Do not pull latest changes from remote
#   --no-restore          Do not return to the original branch afterwards
#   -h, --help            Show this help message
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BRANCH="main"
REMOTE=""
SKIP_PULL=0
NO_RESTORE=0
RELEASE_ARGS=()

usage() {
    sed -n '3,21p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --branch)
            BRANCH="${2:?--branch requires a value}"
            shift
            ;;
        --remote)
            REMOTE="${2:?--remote requires a value}"
            shift
            ;;
        --skip-pull)
            SKIP_PULL=1
            ;;
        --no-restore)
            NO_RESTORE=1
            ;;
        -h|--help)
            usage
            ;;
        *)
            # Forward all other arguments (--build-only, --tag, --draft, etc.) to release.sh
            RELEASE_ARGS+=("$1")
            ;;
    esac
    shift
done

# Detect git remote if not provided
if [[ -z "$REMOTE" ]]; then
    if git remote | grep -qx "github"; then
        REMOTE="github"
    elif git remote | grep -qx "origin"; then
        REMOTE="origin"
    else
        REMOTE="$(git remote | head -n 1)"
    fi
fi

if [[ -z "$REMOTE" ]]; then
    echo "publish.sh: Error: No git remote found in repository." >&2
    exit 1
fi

CURRENT_BRANCH="$(git branch --show-current || true)"

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
    echo "publish.sh: Warning: Working tree has uncommitted changes." >&2
    echo "Please commit or stash your changes before releasing." >&2
    exit 1
fi

echo "========================================================"
echo "  Terminus Release Automation"
echo "  Branch: $BRANCH | Remote: $REMOTE"
echo "========================================================"

if [[ "$SKIP_PULL" == "0" ]]; then
    echo "==> Fetching from $REMOTE..."
    git fetch "$REMOTE" "$BRANCH"

    if [[ "$CURRENT_BRANCH" != "$BRANCH" ]]; then
        echo "==> Switching to branch '$BRANCH'..."
        git checkout "$BRANCH"
    fi

    echo "==> Pulling latest changes from $REMOTE/$BRANCH..."
    git pull --ff-only "$REMOTE" "$BRANCH"
else
    if [[ "$CURRENT_BRANCH" != "$BRANCH" ]]; then
        echo "==> Switching to branch '$BRANCH'..."
        git checkout "$BRANCH"
    fi
fi

VERSION="$(grep -m 1 '^version = ' Cargo.toml | awk -F '"' '{print $2}')"
echo "==> Release target version: $VERSION"

# Execute release build & publish inside the Nix release devShell
echo "==> Running release build & publish..."
if command -v nix >/dev/null 2>&1; then
    nix develop "$ROOT#release" --command bash "$ROOT/scripts/release.sh" "${RELEASE_ARGS[@]}"
else
    bash "$ROOT/scripts/release.sh" "${RELEASE_ARGS[@]}"
fi

# Restore branch if we moved from a different branch
if [[ "$NO_RESTORE" == "0" && -n "$CURRENT_BRANCH" && "$CURRENT_BRANCH" != "$BRANCH" ]]; then
    echo "==> Returning to original branch '$CURRENT_BRANCH'..."
    git checkout "$CURRENT_BRANCH"
fi

echo "========================================================"
echo "  Release process finished successfully!"
echo "========================================================"
