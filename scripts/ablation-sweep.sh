#!/usr/bin/env bash
# Run the galaxy-gen#66 stellar-heating ablation matrix and print one
# vsig-versus-tick table per configuration.
#
# The stellar disk starts rotation-dominated and ends pressure-supported.
# Two candidate fixes were measured against that and neither moved the
# crossover, so the method here is ablation: switch one candidate heat
# source off, re-run, and see whether the crossover moves. Each switch is
# read by the kernel itself (src/rust/ablation.rs) and echoed in the run's
# own header, so what ran and what is reported cannot drift apart.
#
# Usage: just ablation-sweep [ticks] [size] [seeds] [start-seed] [scenario]
# Defaults match the measurements recorded on the issue. Configurations run
# concurrently - each debug_sim process is single-threaded.
set -euo pipefail

TICKS="${1:-2500}"
SIZE="${2:-500}"
SEEDS="${3:-2}"
START_SEED="${4:-12345}"
SCENARIO="${5:-2}"

# name:VAR=value[ VAR=value...]. "baseline" carries no switches.
CONFIGS=(
  "baseline:"
  "fresh-field:GALAXY_ABL_FIELD_CADENCE=1"
  "smoothed-field:GALAXY_ABL_FIELD_SMOOTH=2"
  "axisymmetric-field:GALAXY_ABL_AXISYMMETRIC_FIELD=1"
  "no-star-self-gravity:GALAXY_ABL_NO_STAR_SELF_GRAVITY=1"
  "no-association-binding:GALAXY_ABL_NO_ASSOCIATION_BINDING=1"
  "no-birth-dispersion:GALAXY_ABL_NO_BIRTH_DISPERSION=1"
  "birth-orbit-ratio-cap:GALAXY_ABL_BIRTH_ORBIT_RATIO_CAP=1.06"
)

# Build once so the configurations start together instead of racing cargo.
cargo build --release --bin debug_sim
BIN="${PWD}/target/release/debug_sim"
OUT="$(mktemp -d "${TMPDIR:-/tmp}/galaxy-ablation.XXXXXX")"
trap 'rm -rf "${OUT}"' EXIT

for entry in "${CONFIGS[@]}"; do
  name="${entry%%:*}"
  vars="${entry#*:}"
  # shellcheck disable=SC2086
  env ${vars} "${BIN}" "${TICKS}" "${SIZE}" "${SEEDS}" "${START_SEED}" "${SCENARIO}" \
    >"${OUT}/${name}.txt" 2>&1 &
done
wait

for entry in "${CONFIGS[@]}"; do
  name="${entry%%:*}"
  echo "=== ${name} ==="
  grep -E '^(ablation:|--- |t=)' "${OUT}/${name}.txt" |
    sed -E 's/^(t= *[0-9]+).*(vsig=[0-9.-]+).*(stars= *[0-9]+) +(mixed= *[0-9]+).*/\1  \2  \3  \4/'
  echo
done
