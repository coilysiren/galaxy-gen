#!/usr/bin/env bash
# Forgejo runs this job on the default runner image, which ships node for the
# checkout action but no rust. A `container: rust:*` override would drop node,
# so rustup installs the toolchain into the job instead.
set -euo pipefail

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"
