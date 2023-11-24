.DEFAULT_GOAL := help

help:
	@perl -nle'print $& if m{^[a-zA-Z_-]+:.*?## .*$$}' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

install:
	cargo build
	npm install

test-rust:
	cargo check
	cargo test -- --color always

build-rust:
	cargo build
	make build-wasm

build-rust-dev:
	cargo watch	-s "make build-rust"

build-wasm:
	- cargo install wasm-pack
	wasm-pack init --mode no-install
	npm install ./pkg

build-js-dev:
	npx webpack-serve src/js/webpack.config.js --port 3000

build-js-prod:
	npx webpack-cli --config src/js/webpack.config.js --mode production

deploy-compiled-files:
	bash bin/deploy-compiled-files.sh
