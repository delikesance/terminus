# Distribution binaries must be linked against distro libraries, even when
# release.sh is launched from a Nix shell. Jammy provides glibc 2.35 / GCC 11.
FROM ubuntu:22.04

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    ca-certificates curl build-essential binutils cmake clang libclang-dev \
    pkg-config python3 glslang-tools libfontconfig1-dev libfreetype6-dev \
    libkrb5-dev libssl-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev \
    libxcb1-dev libwayland-dev libxkbcommon-dev libgl1-mesa-dev libvulkan-dev \
    && rm -rf /var/lib/apt/lists/*

ARG RUST_VERSION
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo
ENV PATH=/opt/cargo/bin:${PATH}
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup.sh \
    && sh /tmp/rustup.sh -y --profile minimal --default-toolchain "${RUST_VERSION}" \
    && chmod -R a+rX /opt/rustup /opt/cargo \
    && rm /tmp/rustup.sh

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add --toolchain "${RUST_VERSION}" x86_64-unknown-linux-musl \
    && chmod -R a+rX /opt/rustup

WORKDIR /work
