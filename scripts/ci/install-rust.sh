#!/usr/bin/env bash
# Forgejo runs this job on the default runner image, which ships node for the
# checkout action but no rust. A `container: rust:*` override would drop node,
# so rustup installs the toolchain into the job instead.
set -euo pipefail

# clippy and rustfmt are not in the minimal profile, and the lint gate needs
# both. The version comes from rust-toolchain.toml, not from whatever stable
# happens to be current - rustup honours the pin on the first cargo call.
# Before that pin existed, CI drifted ahead of every developer machine and the
# deny-by-default lint gate went red on new lints nobody could reproduce.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
  sh -s -- -y --profile minimal --component clippy --component rustfmt
echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"
