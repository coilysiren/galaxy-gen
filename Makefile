.DEFAULT_GOAL := help

.PHONY: help install test-rust build-rust build-wasm build-js-prod dev dev-js dev-rust \
        test-e2e test-e2e-ui test build-docker run-docker .build-docker

# --- Config ----------------------------------------------------------------
image    ?= galaxy-gen
git-hash ?= $(shell git rev-parse HEAD 2>/dev/null || echo dev)

help: ## Show this help
	@perl -nle'print $& if m{^[a-zA-Z_-]+:.*?## .*$$}' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

# --- Local dev -------------------------------------------------------------

install: ## Install Rust, WASM, and JS deps (cargo build + wasm-pack + npm install + playwright install).
	cargo build
	cargo install wasm-pack
	wasm-pack build
	npm install
	npx playwright install chromium

test-rust: ## cargo check + cargo test.
	cargo check
	cargo test -- --color always

build-rust: build-wasm ## Build Rust + WASM (debug).
	cargo build

build-wasm: ## Compile Rust to WASM via wasm-pack (pkg/).
	wasm-pack build

build-js-prod: build-wasm ## Production webpack build.
	npx webpack --config webpack.config.js --mode production

# The trailing `touch` forces a webpack recompile: pkg/ lives under
# node_modules (symlink) and webpack's watcher does not rebuild the module
# graph on pkg-only changes, so without it the browser keeps running the
# previous WASM.
WASM_WATCH_CMD = wasm-pack build --dev && touch src/js/index.js

dev: ## Run the rust/wasm watcher and webpack-dev-server concurrently with auto-reload.
	@echo "Starting rust watcher + JS dev server (Ctrl-C stops both)"
	@trap 'kill 0' INT TERM EXIT; \
		cargo watch -w src/rust -w Cargo.toml -s "$(WASM_WATCH_CMD)" & \
		npx webpack serve --open & \
		wait

dev-js: ## Run only the JS dev server with HMR.
	npx webpack serve --open

dev-rust: ## Run only the Rust/WASM watcher.
	cargo watch -w src/rust -w Cargo.toml -s "$(WASM_WATCH_CMD)"

test-e2e: build-wasm ## Run Playwright end-to-end tests.
	npm install ./pkg --no-save
	npx playwright test

test-e2e-ui: build-wasm ## Run Playwright tests in UI mode.
	npm install ./pkg --no-save
	npx playwright test --ui

test: test-rust test-e2e ## Run all tests (rust + e2e)

# --- Docker (local validation of the deploy-built image) -------------------

# The real image is built and rolled out by coilyco-bridge/deploy
# (services/galaxy-gen/scripts/rollout.sh, over this repo's git context on
# kai-server). These targets exist to validate that same build locally.
#
# --platform linux/amd64 is load-bearing: the cluster node (kai-server) is
# amd64 and the build stage hardcodes the x86_64 binaryen tarball, so the
# image is intrinsically amd64. Without this, a build on an arm64 host (Apple
# Silicon + OrbStack) produces a manifest the node rejects at pull time
# with "no match for platform in manifest". See galaxy-gen#23.
.build-docker:
	docker build \
		--platform linux/amd64 \
		--progress plain \
		--build-arg BUILDKIT_INLINE_CACHE=1 \
		--build-arg SENTRY_DSN=$(SENTRY_DSN) \
		--cache-from $(image):latest \
		-t $(image):$(git-hash) \
		-t $(image):latest \
		.

build-docker: .build-docker ## Build the docker image locally with BuildKit cache.

run-docker: ## Serve the locally built image on :8080 (mirrors the k8s Deployment).
	docker run --rm --platform linux/amd64 -p 8080:8080 $(image):latest
