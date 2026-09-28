#!/usr/bin/env bash
#
# Tests for the release signing step (scripts/sign-release.sh) and the
# release tag guard in scripts/release.sh. Needs minisign; builds nothing.
#
#   bash scripts/test-release-signing.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SIGN="$ROOT/scripts/sign-release.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
FAILED=0

pass() { echo "ok   - $1"; }
fail() { echo "FAIL - $1"; FAILED=1; }

minisign -G -W -p "$WORK/release.pub" -s "$WORK/release.key" >/dev/null
minisign -G -W -p "$WORK/other.pub" -s "$WORK/other.key" >/dev/null
: > "$WORK/empty.pub"

new_dist() {
    rm -rf "$WORK/dist"
    mkdir -p "$WORK/dist"
    echo "binary" > "$WORK/dist/terminus-linux-x86_64.tar.gz"
    (cd "$WORK/dist" && sha256sum terminus-linux-x86_64.tar.gz > checksums.txt)
}

# sign <committed public key> <signing key> -> exit status
sign() {
    TERMINUS_UPDATE_PUBKEY_FILE="$1" TERMINUS_SIGNING_KEY="$2" \
        bash "$SIGN" "$WORK/dist" >"$WORK/out" 2>&1
}

new_dist
if sign "$WORK/release.pub" "$WORK/release.key" \
    && minisign -V -q -p "$WORK/release.pub" -m "$WORK/dist/checksums.txt"; then
    pass "signs checksums.txt with the matching key"
else
    fail "signs checksums.txt with the matching key"; cat "$WORK/out"
fi

new_dist
if ! sign "$WORK/release.pub" "$WORK/missing.key" \
    && [[ ! -e "$WORK/dist/checksums.txt.minisig" ]]; then
    pass "refuses to continue unsigned once a public key is committed"
else
    fail "refuses to continue unsigned once a public key is committed"
fi

new_dist
if ! sign "$WORK/release.pub" "$WORK/other.key" \
    && [[ ! -e "$WORK/dist/checksums.txt.minisig" ]]; then
    pass "rejects a signing key that does not match the committed public key"
else
    fail "rejects a signing key that does not match the committed public key"
fi

new_dist
if sign "$WORK/empty.pub" "$WORK/missing.key" \
    && [[ ! -e "$WORK/dist/checksums.txt.minisig" ]] \
    && grep -q "unsigned" "$WORK/out"; then
    pass "without a committed key, releases stay unsigned with a warning"
else
    fail "without a committed key, releases stay unsigned with a warning"; cat "$WORK/out"
fi

new_dist
rm "$WORK/dist/checksums.txt"
if ! sign "$WORK/release.pub" "$WORK/release.key"; then
    pass "fails when checksums.txt is missing"
else
    fail "fails when checksums.txt is missing"
fi

# The updater compares the release tag with the version compiled into the
# binary: a mismatch would make every install re-download the update.
if ! bash "$ROOT/scripts/release.sh" --build-only --tag v0.0.0-mismatch \
    >"$WORK/out" 2>&1 && grep -q "does not match" "$WORK/out"; then
    pass "release.sh rejects a tag that differs from the Cargo version"
else
    fail "release.sh rejects a tag that differs from the Cargo version"; cat "$WORK/out"
fi

# Publishing again under a tag that already has a release used to replace
# its files silently. It must stop before building, unless --replace.
mkdir -p "$WORK/bin"
cat > "$WORK/bin/gh" <<'GH'
#!/bin/sh
# `gh release view` succeeds: the release exists. Anything else is a bug.
[ "$1 $2" = "release view" ] && exit 0
echo "unexpected gh $*" >&2; exit 97
GH
cat > "$WORK/bin/cargo" <<'CARGO'
#!/bin/sh
echo "cargo must not run" >&2; exit 98
CARGO
chmod +x "$WORK/bin/gh" "$WORK/bin/cargo"
VERSION="$(grep -m 1 '^version = ' "$ROOT/Cargo.toml" | awk -F '"' '{print $2}')"
PATH="$WORK/bin:$PATH" bash "$ROOT/scripts/release.sh" >"$WORK/out" 2>&1
status=$?
if [[ $status -ne 0 ]] && grep -q "already published" "$WORK/out" \
    && grep -q "prepare-release.sh" "$WORK/out" && ! grep -q "cargo must not run" "$WORK/out"; then
    pass "release.sh refuses to overwrite v$VERSION before building"
else
    fail "release.sh refuses to overwrite v$VERSION before building"; cat "$WORK/out"
fi
PATH="$WORK/bin:$PATH" bash "$ROOT/scripts/release.sh" --replace >"$WORK/out" 2>&1
if grep -q "cargo must not run" "$WORK/out"; then
    pass "--replace goes on to build"
else
    fail "--replace goes on to build"; cat "$WORK/out"
fi

exit "$FAILED"
