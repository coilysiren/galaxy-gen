.DEFAULT_GOAL := help

help:
	@perl -nle'print $& if m{^[a-zA-Z_-]+:.*?## .*$$}' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

watch: ## watch (primary entrypoint)
	npx concurrently \
		-k -n rust,rust-tests,js,js-tests \
		-c red,red,green,green \
		"make watch-rust" \
		"make watch-rust-tests" \
		"make watch-js" \
		"make watch-js-tests"

watch-rust:
	cargo +nightly watch \
		-x "build --target wasm32-unknown-unknown" \
		-s "wasm-bindgen target/wasm32-unknown-unknown/debug/galaxy_gen.wasm --typescript --debug --out-dir src/js/assets/built-wasm --browser"

watch-rust-tests:
	cargo +nightly watch \
		-x "check" \
		-x "test -- --color always --nocapture"

watch-js:
	npx webpack-serve src/js/webpack.config.js

watch-js-tests:
	npx karma start src/js/tests/karma.conf.js

build: ## build (outdated, kept for reference)
	cargo +nightly build --release --target wasm32-unknown-unknown && wasm-bindgen target/wasm32-unknown-unknown/release/galaxy_gen.wasm --out-dir src/js/assets/built-wasm
