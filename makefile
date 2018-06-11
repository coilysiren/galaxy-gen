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

install:
	cargo build
	npm install

dev: ## dev (primary entrypoint)
	npx concurrently \
		-k -n rust,rust::test,js,js::test \
		-c red,red,green,green \
		"make rust-build-dev" \
		"make rust-test" \
		"make js-build-dev" \
		"make js-test"

test-rust:
	cargo check
	cargo test -- --color always

test-rust-watch:
	cargo watch -s "make test-rust"

test-js:
	npx karma start src/js/tests/karma.conf.js

build-all-prod:
	make rust-build-prod
	make js-build-prod

build-rust-dev:
	cargo watch -x "build"

build-wasm:
	- cargo install wasm-pack
	wasm-pack init
	# npm install ./pkg

build-js-dev:
	npx webpack-serve src/js/webpack.config.js --port 3000

build-js-prod:
	npx webpack-cli --config src/js/webpack.config.js --mode production

deploy-wasm:
	bash bin/deploy-wasm-pack.sh
