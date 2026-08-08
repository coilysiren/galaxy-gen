#!/usr/bin/env bash
set -euo pipefail

build_wasm() {
  wasm-pack build
}

test_rust() {
  cargo check
  cargo test -- --color always
}

# The single definition of the Rust lint gate. CI calls this script rather than
# repeating the flags, so local and CI cannot drift apart.
lint_rust() {
  cargo clippy --all-targets -- -D warnings
  cargo fmt --check
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
  deps-sync)
    npm install --package-lock-only
    ;;
  test-rust)
    test_rust
    ;;
  format-rust)
    cargo fmt
    ;;
  lint-rust)
    lint_rust
    ;;
  build-rust)
    build_wasm
    cargo build
    ;;
  build-wasm)
    build_wasm
    ;;
  debug-sim)
    shift
    cargo run --release --bin debug_sim -- "$@"
    ;;
  build-js-prod)
    build_wasm
    npx webpack --config webpack.config.js --mode production
    ;;
  check-js)
    npm run lint
    npm run typecheck
    ;;
  capture-readme)
    node scripts/capture-readme.mjs
    ;;
  promote-readme)
    candidate="${GALAXY_CAPTURE_OUTPUT:-docs/project-galaxy-gen.next.gif}"
    tracked="docs/project-galaxy-gen.gif"
    if [[ "${candidate}" == "${tracked}" || ! -f "${candidate}" ]]; then
      echo "capture candidate not found or not safely separated: ${candidate}" >&2
      exit 2
    fi
    mv -f -- "${candidate}" "${tracked}"
    rm -f -- docs/project-galaxy-gen.next.gif docs/project-galaxy-gen.next2.gif
    ;;
  dev)
    echo "Starting rust watcher + JS dev server (Ctrl-C stops both)"
    trap 'kill 0' INT TERM EXIT
    cargo watch -w src/rust -w Cargo.toml -s "wasm-pack build && touch src/js/index.js" &
    webpack_args=(serve --open)
    if [[ -n "${GALAXY_DEV_PORT:-}" ]]; then
      webpack_args+=(--port "${GALAXY_DEV_PORT}")
    fi
    npx webpack "${webpack_args[@]}" &
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
  perf-profile)
    shift
    cargo run --release --bin perf_profile -- "$@"
    ;;
  test-perf)
    # Real GPU, not the default config's SwiftShader: Canvas2D cost
    # attribution is meaningless under software rasterization.
    build_wasm
    npm install ./pkg --no-save
    npx playwright test --config playwright.gpu.config.ts --headed --workers=1 \
      render-perf runtime-perf
    ;;
  test-e2e-ui)
    build_wasm
    npm install ./pkg --no-save
    npx playwright test --ui
    ;;
  test)
    lint_rust
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
