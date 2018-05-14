.DEFAULT_GOAL := help

help:
	@perl -nle'print $& if m{^[a-zA-Z_-]+:.*?## .*$$}' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

update:
	brew upgrade
	brew update
	rustup self update
	rustup update

install:
	cargo update
	npm install

dev: ## dev (primary entrypoint)
	npx concurrently \
		-k -n rust,rust::test,js,js::test \
		-c red,red,green,green \
		"make rust-build-dev" \
		"make rust-test" \
		"make js-build-dev" \
		"make js-test"

all-build-prod:
	make rust-build-prod
	make js-build-prod

rust-build-dev:
	cargo +nightly watch \
		-x "build --target wasm32-unknown-unknown" \
		-s "make wasm-build-dev"

rust-build-prod:
	cargo +nightly build --release --target wasm32-unknown-unknown
	make wasm-build-prod

rust-test:
	cargo +nightly watch \
		-x "check" \
		-x "test -- --color always --nocapture"

wasm-build-dev:
	wasm-bindgen target/wasm32-unknown-unknown/debug/galaxy_gen.wasm --typescript --debug --out-dir src/js/assets/built-wasm --browser

wasm-build-prod:
	wasm-bindgen target/wasm32-unknown-unknown/debug/galaxy_gen.wasm --out-dir src/js/assets/built-wasm --browser

js-build-dev:
	npx webpack-serve src/js/webpack.config.js --port 3000

js-build-prod:
	npx webpack-cli --config src/js/webpack.config.js --mode production

js-test:
	npx karma start src/js/tests/karma.conf.js
