watch:
	cargo +nightly watch \
		-x "build --target wasm32-unknown-unknown" \
		-s "wasm-bindgen target/wasm32-unknown-unknown/debug/galaxy_gen.wasm --typescript --debug --out-dir client/assets/built-wasm" \
		-s "npx --no-install webpack-dev-server"

build:
	cargo +nightly build --release --target wasm32-unknown-unknown && wasm-bindgen target/wasm32-unknown-unknown/release/galaxy_gen.wasm --out-dir client/assets/built-wasm
