#!/usr/bin/env bash
# Install the built packages in clean distro containers and exercise the loader.
# Run after scripts/release.sh --build-only --linux-only. Requires Docker.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
test -f "$ROOT/dist/terminus_${VERSION}_amd64.deb"
test -f "$ROOT/dist/terminus-${VERSION}-1.x86_64.rpm"

docker run --rm --platform linux/amd64 \
    --mount "type=bind,src=$ROOT/dist,dst=/packages,readonly" \
    -e "VERSION=$VERSION" ubuntu:22.04 bash -euc '
        apt-get update >/tmp/install.log 2>&1
        DEBIAN_FRONTEND=noninteractive apt-get install -y "/packages/terminus_${VERSION}_amd64.deb" >>/tmp/install.log 2>&1 || {
            cat /tmp/install.log; exit 1;
        }
        terminus --version
        ldd /usr/bin/terminus
        if ldd /usr/bin/terminus | grep -q "not found"; then exit 1; fi
    '
docker run --rm --platform linux/amd64 \
    --mount "type=bind,src=$ROOT/dist,dst=/packages,readonly" \
    -e "VERSION=$VERSION" fedora:44 bash -euc '
        dnf -y install "/packages/terminus-${VERSION}-1.x86_64.rpm" >/tmp/install.log 2>&1 || {
            cat /tmp/install.log; exit 1;
        }
        terminus --version
        ldd /usr/bin/terminus
        if ldd /usr/bin/terminus | grep -q "not found"; then exit 1; fi
    '
