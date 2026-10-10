#!/usr/bin/env bash
#
# Increment the patch number of the workspace version (Cargo.toml and the
# internal path dependencies pinned to it) and refresh Cargo.lock.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

current="$(grep -m 1 '^version = ' Cargo.toml | awk -F '"' '{print $2}')"
IFS=. read -r major minor patch <<<"$current"
next="${major}.${minor}.$((patch + 1))"

sed -i -E \
    -e "0,/^version = \"${current}\"/s//version = \"${next}\"/" \
    -e "s/(path = \"[^\"]+\", version = \")${current}\"/\1${next}\"/" \
    Cargo.toml
cargo update --workspace --offline >/dev/null 2>&1

echo "version ${current} -> ${next}"
