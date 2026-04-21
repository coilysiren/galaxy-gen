# Development

## Architecture

### Rust Backend (`src/rust/`)

- `lib.rs` — crate root, re-exports the `galaxy` module
- `galaxy.rs` — core simulation logic: `Galaxy` struct with `Cell` grid, gravitational physics (Newton's law), acceleration, seeding, tick advancement. Exposed to JS via `wasm-bindgen`

The Galaxy is immutable-style — methods like `seed()`, `tick()` return new Galaxy instances.

### WASM Bridge

- Built with `wasm-pack`, output goes to `pkg/` (gitignored)
- The JS package.json references `"galaxy_gen_backend": "file:pkg"` as a dev dependency

### JavaScript Frontend (`src/js/`)

- `index.html` — Bootstrap 5 dark theme shell
- `index.js` — React entry point
- `lib/galaxy.ts` — `Frontend` class wrapping the WASM Galaxy, exposes `seed()`, `tick()`, `cells()`
- `lib/application.tsx` — React UI with inputs (galaxy size, seed mass, time modifier) and buttons (init, seed, advance time)
- `lib/dataviz.tsx` — D3 scatter plot visualization of cells, circle radius = log(mass)
- `lib/styles.css` — custom styles

### Build System

- Rust: `cargo build`, `cargo test`
- WASM: `wasm-pack build` (previously `wasm-pack init`)
- JS: webpack 5 with babel (React + TypeScript presets), dev server via `webpack-dev-server`
- `makefile` has convenience targets but some are outdated (references `wasm-pack init`)

### CI

- GitHub Actions (`.github/workflows/action.yml`): two jobs — `rust` (build/check/test + wasm-pack) and `js` (wasm-pack + npm ci)

## Commands

```bash
# Rust
cargo build          # compile
cargo check          # type check
cargo test           # run tests (there are many in galaxy.rs)

# WASM
wasm-pack build      # compile to WASM, output in pkg/

# JS
npm install          # install deps (requires pkg/ from wasm-pack)
npm run build        # production webpack build
npm start            # dev server
```

## Key Conventions

- Rust code uses `wasm_bindgen` for the public API boundary; private methods are plain `impl` blocks
- Galaxy grid is flat `Vec<Cell>` indexed by `row * size + col`
- Physics uses magnitude + degrees (not x/y vectors) for acceleration storage, converting at computation boundaries
- Tests are organized in `mod tests_*` blocks at the bottom of `galaxy.rs`
- Frontend state is managed with React `useState` hooks (no state library)
- No linting/formatting tools are actively enforced beyond `tslint.json` (which is deprecated)

## Dependencies

- Rust: `wasm-bindgen`, `specs`/`specs-derive` (ECS, currently unused), `rand`, `console_error_panic_hook`
- JS: React 18, D3 7, TypeScript 5, webpack 5, Bootstrap 5 (CDN)
