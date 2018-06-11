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

all-build-prod:
	make rust-build-prod
	make js-build-prod

rust-build-dev:
	cargo watch \
		-x "build" \
		-s "./bin/post_compile"

rust-build:
	rm -rf pkg
	cargo install wasm-pack --force
	wasm-pack init -vv
	npm install ./pkg

rust-test:
	cargo watch \
		-x "check" \
		-x "test -- --color always --nocapture"

js-build-dev:
	npx webpack-serve src/js/webpack.config.js --port 3000

js-build-prod:
	npx webpack-cli --config src/js/webpack.config.js --mode production

js-test:
	npx karma start src/js/tests/karma.conf.js
