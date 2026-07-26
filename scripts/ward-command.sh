#!/usr/bin/env bash
set -euo pipefail

build_wasm() {
  wasm-pack build
}

test_rust() {
  cargo check
  cargo test -- --color always
}

test_e2e() {
  build_wasm
  npm install ./pkg --no-save
  npx playwright test
}

case "${1:-}" in
  install)
    cargo build
    cargo install wasm-pack
    build_wasm
    npm install
    npx playwright install chromium
    ;;
  test-rust)
    test_rust
    ;;
  build-rust)
    build_wasm
    cargo build
    ;;
  build-wasm)
    build_wasm
    ;;
  build-js-prod)
    build_wasm
    npx webpack --config webpack.config.js --mode production
    ;;
  dev)
    echo "Starting rust watcher + JS dev server (Ctrl-C stops both)"
    trap 'kill 0' INT TERM EXIT
    cargo watch -w src/rust -w Cargo.toml -s "wasm-pack build && touch src/js/index.js" &
    npx webpack serve --open &
    wait
    ;;
  dev-js)
    npx webpack serve --open
    ;;
  dev-rust)
    cargo watch -w src/rust -w Cargo.toml -s "wasm-pack build && touch src/js/index.js"
    ;;
  test-e2e)
    test_e2e
    ;;
  test-e2e-ui)
    build_wasm
    npm install ./pkg --no-save
    npx playwright test --ui
    ;;
  test)
    test_rust
    test_e2e
    ;;
  build-docker)
    image="${IMAGE_NAME:-galaxy-gen}"
    git_hash="${GIT_HASH:-$(git rev-parse HEAD 2>/dev/null || echo dev)}"
    docker build \
      --platform linux/amd64 \
      --progress plain \
      --build-arg BUILDKIT_INLINE_CACHE=1 \
      --build-arg "SENTRY_DSN=${SENTRY_DSN:-}" \
      --cache-from "${image}:latest" \
      -t "${image}:${git_hash}" \
      -t "${image}:latest" \
      .
    ;;
  run-docker)
    image="${IMAGE_NAME:-galaxy-gen}"
    docker run --rm --platform linux/amd64 -p 8080:8080 "${image}:latest"
    ;;
  *)
    echo "unknown Ward action: ${1:-}" >&2
    exit 2
    ;;
esac
