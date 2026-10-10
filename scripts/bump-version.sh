#!/usr/bin/env bash
#
# Bump the workspace patch version via misc/prepare-release.sh and commit the
# result, so the release tag carries its own version bump.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

current="$(grep -m 1 '^version = ' Cargo.toml | awk -F '"' '{print $2}')"
IFS=. read -r major minor patch <<<"$current"
next="${major}.${minor}.$((patch + 1))"

misc/prepare-release.sh "$next"
git add Cargo.toml Cargo.lock misc/com.rioterm.Rio.metainfo.xml
git commit -m "chore: release v${next}"
