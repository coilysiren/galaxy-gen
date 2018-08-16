.DEFAULT_GOAL := help

help:
	@perl -nle'print $& if m{^[a-zA-Z_-]+:.*?## .*$$}' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

update:
	brew upgrade
	brew update
	rustup self update
	rustup update
	rm Cargo.lock
	cargo update
	npm i -g npm

install:
	cargo build
	npm install

dev: ## dev (primary entrypoint)
	npx concurrently \
		-k -n rust,rust::test,js,js::test \
		-c red,red,green,green \
		"make build-rust-dev" \
		"make test-rust-dev" \
		"make build-js-dev" \
		"make test-js-dev"

test-rust:
	cargo check
	cargo test -- --color always

test-rust-dev:
	cargo watch -s "make test-rust"

test-js:
	npx karma start src/js/tests/karma.conf.js --singleRun=true

test-js-dev:
	npx karma start src/js/tests/karma.conf.js

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
