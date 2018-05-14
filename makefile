.DEFAULT_GOAL := help

help:
	@perl -nle'print $& if m{^[a-zA-Z_-]+:.*?## .*$$}' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

install:
	cargo update
	npm install

start: ## start (primary entrypoint)
	npx concurrently \
		-k -n rust,rust-test,js,js-test \
		-c red,red,green,green \
		"make rust-build" \
		"make rust-test" \
		"make js-build" \
		"make js-test"

rust-build:
	cargo +nightly watch \
		-x "build --target wasm32-unknown-unknown" \
		-s "wasm-bindgen target/wasm32-unknown-unknown/debug/galaxy_gen.wasm --typescript --debug --out-dir src/js/assets/built-wasm --browser"

rust-test:
	cargo +nightly watch \
		-x "check" \
		-x "test -- --color always --nocapture"

js-build:
	npx webpack-serve src/js/webpack.config.js

js-test:
	npx karma start src/js/tests/karma.conf.js

build: ## build (outdated, kept for reference)
	cargo +nightly build --release --target wasm32-unknown-unknown && wasm-bindgen target/wasm32-unknown-unknown/release/galaxy_gen.wasm --out-dir src/js/assets/built-wasm
