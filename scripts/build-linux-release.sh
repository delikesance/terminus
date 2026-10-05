#!/usr/bin/env bash
# Build a distro-linked binary without inheriting Nix's compiler, libraries,
# flags or target cache. Docker is also usable from the Nix release shell.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_VERSION="$(sed -n 's/^channel = "\([^"]*\)"/\1/p' "$ROOT/rust-toolchain.toml")"
IMAGE="terminus-linux-release:${RUST_VERSION}"
BUILD_DIR="$ROOT/target/linux-release"
CACHE_DIR="$ROOT/.dev/linux-release-cargo"

if ! command -v docker >/dev/null 2>&1; then
    echo "build-linux-release.sh: Docker is required for distro-compatible Linux releases" >&2
    exit 1
fi
mkdir -p "$BUILD_DIR" "$CACHE_DIR"
docker build --platform linux/amd64 --build-arg "RUST_VERSION=$RUST_VERSION" \
    -t "$IMAGE" -f "$ROOT/misc/linux-release.Dockerfile" "$ROOT/misc"
docker run --rm --platform linux/amd64 --user "$(id -u):$(id -g)" \
    --mount "type=bind,src=$ROOT,dst=/work,readonly" \
    --mount "type=bind,src=$BUILD_DIR,dst=/build" \
    --mount "type=bind,src=$CACHE_DIR,dst=/cargo" \
    -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/build \
    -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    -e 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS=-C target-feature=+crt-static -C link-arg=-static' \
    -e "RUST_VERSION=$RUST_VERSION" \
    -e "CARGO_PROFILE_RELEASE_LTO=${CARGO_PROFILE_RELEASE_LTO:-false}" \
    -e "CARGO_PROFILE_RELEASE_CODEGEN_UNITS=${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-16}" \
    -e "CARGO_PROFILE_RELEASE_DEBUG=${CARGO_PROFILE_RELEASE_DEBUG:-0}" \
    "$IMAGE" bash -euc '
        cargo +"$RUST_VERSION" build --locked --release -p terminus-walk --target x86_64-unknown-linux-musl
        helper=/build/x86_64-unknown-linux-musl/release/terminus-walk
        test -s "$helper"
        if readelf --program-headers "$helper" | grep -q INTERP; then
            echo "Linux release rejected: remote walk helper must be statically linked" >&2
            exit 1
        fi
        cargo +"$RUST_VERSION" build --locked --release -p rioterm --features wgpu --target x86_64-unknown-linux-gnu
        binary=/build/x86_64-unknown-linux-gnu/release/terminus
        python3 /work/scripts/check-linux-release.py "$binary"
        "$binary" --version
    '
