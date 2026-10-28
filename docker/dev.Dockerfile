# Development and Linux build environment for OpenKeyForge.
#
# Everything (Rust, Node, the WebKitGTK headers Tauri links against, the
# bundlers for .deb/.rpm/AppImage) lives in this image so nothing has to be
# installed on the host. Matches the ubuntu-24.04 runner's package set in
# .github/workflows/build.yml closely enough that a green build here means a
# green Linux build there.
#
#   docker build -f docker/dev.Dockerfile -t okf-dev .
#   docker run --rm -it -v "${PWD}:/work" -v okf-target:/work/target -v okf-npm:/work/node_modules okf-dev bash

FROM rust:1-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
        libwebkit2gtk-4.1-dev \
        libgtk-3-dev \
        libayatana-appindicator3-dev \
        librsvg2-dev \
        libudev-dev \
        libxdo-dev \
        libssl-dev \
        pkg-config \
        patchelf \
        rpm \
        file \
        xdg-utils \
        xvfb \
        xauth \
        llvm \
    && ln -s "$(ls /usr/bin/llvm-rc* | head -1)" /usr/local/bin/llvm-rc \
    && rm -rf /var/lib/apt/lists/*

# Lets `cargo check --target x86_64-pc-windows-msvc` type-check the
# Windows-only code (global hotkeys) from Linux; llvm-rc above compiles the
# Windows resource file.
RUN rustup target add x86_64-pc-windows-msvc

# Node from the official image rather than a distro package, so the version
# matches CI (setup-node 24).
COPY --from=node:24-bookworm /usr/local/bin/node /usr/local/bin/node
COPY --from=node:24-bookworm /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -s ../lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
    && ln -s ../lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx \
    && node --version && npm --version

RUN rustup component add clippy rustfmt

ENV CARGO_TARGET_DIR=/work/target \
    APPIMAGE_EXTRACT_AND_RUN=1
WORKDIR /work
