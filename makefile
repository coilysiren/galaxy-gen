.DEFAULT_GOAL := help

help:
	@perl -nle'print $& if m{^[a-zA-Z_-]+:.*?## .*$$}' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

watch: ## watch
	make watch-rust &
	make watch-client &
	make watch-js-tests &

watch-rust-tests:
	cargo +nightly watch \
		-x "test -- --nocapture"

watch-rust:
	cargo +nightly watch \
		-x "check" \
		-x "test" \
		-x "build --target wasm32-unknown-unknown" \
		-s "wasm-bindgen target/wasm32-unknown-unknown/debug/galaxy_gen.wasm --typescript --debug --out-dir client/assets/built-wasm --browser"

watch-client:
	npx --no-install webpack-serve

watch-js-tests:
	npx --no-install karma start client/tests/karma.conf.js

build: ## build
	cargo +nightly build --release --target wasm32-unknown-unknown && wasm-bindgen target/wasm32-unknown-unknown/release/galaxy_gen.wasm --out-dir client/assets/built-wasm
