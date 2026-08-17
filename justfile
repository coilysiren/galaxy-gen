# Per-repo task manifest. Run `just` (or `just --list`) to see every verb.
#
# Recipes take trailing arguments directly: `just <verb> a b`, where the
# retired form was `ward exec <verb> -- a b`.
#
# One line of comment per recipe on purpose: just reads only the LAST comment
# line above a recipe, so a wrapped description silently truncates to its tail.
#
# `ward exec` is retired. `.ward/ward.yaml` survives carrying catalog metadata
# only, because the catalog hooks upstream in agentic-os pin that exact path.

set positional-arguments

# Default target: list every available recipe.
default:
    @just --list --unsorted

# Install Rust, WASM, and JS deps (cargo build + wasm-pack + npm install + playwright install).
install *ARGS:
    @bash scripts/ward-command.sh install "$@"

# Refresh package-lock.json after an intentional package.json change.
deps-sync *ARGS:
    @bash scripts/ward-command.sh deps-sync "$@"

# CI dependency setup: lockfile-exact npm ci plus the wasm package. Takes Rust, Node, wasm-pack, and binaryen from the dev-base image instead of from install.
ci-setup *ARGS:
    @bash scripts/ward-command.sh ci-setup "$@"

# cargo check + cargo test.
test-rust *ARGS:
    @bash scripts/ward-command.sh test-rust "$@"

# Format Rust sources with rustfmt.
format-rust *ARGS:
    @bash scripts/ward-command.sh format-rust "$@"

# The Rust lint gate: clippy across all targets with warnings denied, plus a rustfmt check. CI runs this same script.
lint-rust *ARGS:
    @bash scripts/ward-command.sh lint-rust "$@"

# Build Rust + WASM (debug).
build-rust *ARGS:
    @bash scripts/ward-command.sh build-rust "$@"

# Compile Rust to WASM via wasm-pack (pkg/).
build-wasm *ARGS:
    @bash scripts/ward-command.sh build-wasm "$@"

# Run the native seeded physics probe with ticks, size, seed-count, and start-seed arguments.
debug-sim *ARGS:
    @bash scripts/ward-command.sh debug-sim "$@"

# Run the stellar-heating ablation matrix (galaxy-gen#66) and print vsig per configuration.
ablation-sweep *ARGS:
    @bash scripts/ablation-sweep.sh "$@"

# Production webpack build.
build-js-prod *ARGS:
    @bash scripts/ward-command.sh build-js-prod "$@"

# Check JS/TS lint rules and types.
check-js *ARGS:
    @bash scripts/ward-command.sh check-js "$@"

# Format explicitly named JS, TS, CSS, JSON, or Markdown files.
format-files *ARGS:
    @npx prettier --write "$@"

# Capture the fixed-seed README GIF candidate from a running dev server.
capture-readme *ARGS:
    @bash scripts/ward-command.sh capture-readme "$@"

# Replace the tracked README GIF with an inspected capture candidate.
promote-readme *ARGS:
    @bash scripts/ward-command.sh promote-readme "$@"

# Run the Rust/WASM watcher and webpack dev server. GALAXY_DEV_PORT overrides port 8081.
dev *ARGS:
    @bash scripts/ward-command.sh dev "$@"

# Run only the JS dev server with HMR.
dev-js *ARGS:
    @bash scripts/ward-command.sh dev-js "$@"

# Run only the Rust/WASM watcher.
dev-rust *ARGS:
    @bash scripts/ward-command.sh dev-rust "$@"

# Run Playwright end-to-end tests.
test-e2e *ARGS:
    @bash scripts/ward-command.sh test-e2e "$@"

# Attribute one native tick to each process in the registry. Takes size and tick count.
perf-profile *ARGS:
    @bash scripts/ward-command.sh perf-profile "$@"

# Run the render-frame and live frame-pacing probes against system Chrome on real GPU.
test-perf *ARGS:
    @bash scripts/ward-command.sh test-perf "$@"

# Run Playwright tests in UI mode.
test-e2e-ui *ARGS:
    @bash scripts/ward-command.sh test-e2e-ui "$@"

# Build the docker image locally with BuildKit cache.
build-docker *ARGS:
    @bash scripts/ward-command.sh build-docker "$@"

# Parse the trusted Forgejo OCI publisher shell contract.
image-publish-check *ARGS:
    @bash -n scripts/publish-image.sh "$@"

# Serve the locally built image on :8080 (mirrors the k8s Deployment).
run-docker *ARGS:
    @bash scripts/ward-command.sh run-docker "$@"

# Run the Rust and Playwright test suites.
test *ARGS:
    @bash scripts/ward-command.sh test "$@"

# Run all pre-commit hooks against every file.
precommit *ARGS:
    @pre-commit run --all-files "$@"
