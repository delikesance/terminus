#!/usr/bin/env bash
#
# Sign <dist>/checksums.txt for the in-app updater:
#
#   scripts/sign-release.sh dist
#
# Writes <dist>/checksums.txt.minisig. Terminus only installs a release whose
# checksums verify against the public key compiled into it
# (crates/terminus-update/update-public-key.txt).
#
# TERMINUS_SIGNING_KEY          minisign secret key (default ~/.minisign/terminus.key)
# TERMINUS_UPDATE_PUBKEY_FILE   committed public key (default: the file above)
# TERMINUS_RELEASE_TAG          tag recorded in the signature's trusted comment
#
# While no public key is committed, releases stay unsigned and the app only
# announces updates. Once one is committed, signing is mandatory and the
# signature is checked against it, so a wrong key cannot ship.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="${1:?usage: sign-release.sh <dist dir>}"
PUBKEY_FILE="${TERMINUS_UPDATE_PUBKEY_FILE:-$ROOT/crates/terminus-update/update-public-key.txt}"
SECRET_KEY="${TERMINUS_SIGNING_KEY:-$HOME/.minisign/terminus.key}"
CHECKSUMS="$DIST/checksums.txt"
SIGNATURE="$CHECKSUMS.minisig"

if [[ ! -f "$CHECKSUMS" ]]; then
    echo "sign-release.sh: $CHECKSUMS not found" >&2
    exit 1
fi
rm -f "$SIGNATURE"

if ! grep -q '[^[:space:]]' "$PUBKEY_FILE" 2>/dev/null; then
    echo "sign-release.sh: no public key in $PUBKEY_FILE; publishing unsigned" >&2
    echo "  (installed copies will announce this release but not install it)" >&2
    exit 0
fi

if ! command -v minisign >/dev/null 2>&1; then
    echo "sign-release.sh: minisign is required to sign releases" >&2
    exit 1
fi
if [[ ! -f "$SECRET_KEY" ]]; then
    echo "sign-release.sh: signing key $SECRET_KEY not found (set TERMINUS_SIGNING_KEY)" >&2
    exit 1
fi

# The key file may hold the whole minisign .pub file or just its key line.
PUBKEY="$(grep -v '^untrusted comment:' "$PUBKEY_FILE" | grep -m 1 '[^[:space:]]' | tr -d '[:space:]')"

minisign -S -s "$SECRET_KEY" -m "$CHECKSUMS" -x "$SIGNATURE" \
    -t "terminus ${TERMINUS_RELEASE_TAG:-release} checksums"
if ! minisign -V -q -P "$PUBKEY" -m "$CHECKSUMS" -x "$SIGNATURE"; then
    rm -f "$SIGNATURE"
    echo "sign-release.sh: $SECRET_KEY does not match the public key in $PUBKEY_FILE" >&2
    exit 1
fi
echo "Signed $CHECKSUMS"
