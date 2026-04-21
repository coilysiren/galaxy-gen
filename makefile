.DEFAULT_GOAL := help

help: ## Show this help
	@perl -nle'print $& if m{^[a-zA-Z_-]+:.*?## .*$$}' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

install: ## Install Rust, WASM, and JS deps
	cargo build
	cargo install wasm-pack
	wasm-pack build
	npm install
	npx playwright install chromium

test-rust: ## Rust type check + unit tests
	cargo check
	cargo test -- --color always

build-rust: build-wasm ## Build rust + wasm
	cargo build

build-wasm: ## Compile Rust to WASM (pkg/)
	wasm-pack build

build-js-prod: build-wasm ## Production webpack build
	npx webpack --config webpack.config.js --mode production

dev: ## Run rust/wasm watcher and webpack-dev-server concurrently (auto-reload)
	@echo "Starting rust watcher + JS dev server (Ctrl-C stops both)"
	@trap 'kill 0' INT TERM EXIT; \
		cargo watch -w src/rust -w Cargo.toml -s "wasm-pack build --dev" & \
		npx webpack serve --open & \
		wait

dev-js: ## Run only the JS dev server with HMR
	npx webpack serve --open

dev-rust: ## Run only the Rust/WASM watcher (rebuild on change)
	cargo watch -w src/rust -w Cargo.toml -s "wasm-pack build --dev"

test-e2e: build-wasm ## Run Playwright end-to-end tests
	npm install ./pkg --no-save
	npx playwright test

test-e2e-ui: build-wasm ## Run Playwright tests in UI mode
	npm install ./pkg --no-save
	npx playwright test --ui

test: test-rust test-e2e ## Run all tests (rust + e2e)

deploy-compiled-files:
	bash bin/deploy-compiled-files.sh
