#!/usr/bin/env bash
# Ubuntu's apt binaryen produces WASM that trips
# `WebAssembly.Table.grow(): failed to grow table by 4` in chromium, so pin to
# an upstream release binary instead.
set -euo pipefail

VER="${BINARYEN_VERSION:-version_119}"
url="https://github.com/WebAssembly/binaryen/releases/download/${VER}/binaryen-${VER}-x86_64-linux.tar.gz"

curl -sSL "$url" -o /tmp/binaryen.tgz
sudo tar -xzf /tmp/binaryen.tgz -C /usr/local --strip-components=1
wasm-opt --version
