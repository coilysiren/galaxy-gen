# Development

## Architecture

### Rust Backend (`src/rust/`)

- `lib.rs` — crate root, re-exports the `galaxy` module
- `galaxy.rs` - core simulation logic: struct-of-arrays gas grid, gravitational physics, living-galaxy lifecycle, seeding, and tick advancement. Exposed to JS via `wasm-bindgen`

The Galaxy is immutable-style — methods like `seed()`, `tick()` return new Galaxy instances.

### WASM Bridge

- Built with `wasm-pack`, output goes to `pkg/` (gitignored)
- The JS package.json references `"galaxy_gen_backend": "file:pkg"` as a dev dependency

### JavaScript Frontend (`src/js/`)

- `index.html` - browser shell
- `index.js` - React entry point
- `lib/galaxy.ts` - `Frontend` class wrapping the WASM Galaxy and its worker snapshots
- `lib/application.tsx` - React UI and live simulation controls
- `lib/dataviz.tsx` - layered canvas visualization
- `lib/styles.css` - Tailwind theme and custom styles

### Build System

- Rust: `cargo build`, `cargo test`
- WASM: `wasm-pack build` (previously `wasm-pack init`)
- JS: webpack 5 with babel (React + TypeScript presets), dev server via `webpack-dev-server`
- Ward exposes the supported multi-step workflows from `.ward/ward.yaml`.

### CI

- Forgejo (`.forgejo/workflows/ci.yml` on PRs, `build-publish.yml` on `main`): the Rust and JS gates, run inside the dev-base image through `ward exec` verbs. The image supplies rust, node, wasm-pack, and a pinned binaryen, so CI installs no toolchain and the gate matches what `docker build` ships.
- GitHub Actions (`.github/workflows/action.yml`): browser e2e, which stays there because the in-cluster runner cannot reach the Playwright browser CDN. Its `rust` and `js` jobs now duplicate the Forgejo gate and are queued for removal (galaxy-gen#74).

## Commands

```bash
ward exec install
ward exec dev
ward exec test
ward exec build-js-prod
```

To refresh the README animation, start the dev server and run `ward exec capture-readme`. Set `GALAXY_CAPTURE_URL` when the server is not on port 8081. The command writes `docs/project-galaxy-gen.next.gif` and refuses to overwrite either an earlier candidate or the tracked GIF. After inspection, `ward exec promote-readme` replaces the tracked asset with that candidate.

## Key Conventions

- Rust code uses `wasm_bindgen` for the public API boundary; private methods are plain `impl` blocks
- Galaxy state uses parallel flat arrays indexed by `row * size + col`
- Physics accumulates cartesian acceleration and preserves fractional gas positions between grid transfers
- Tests are organized in `mod tests_*` blocks at the bottom of `galaxy.rs`
- Frontend state is managed with React `useState` hooks (no state library)
- ESLint, Prettier, TypeScript, Rust formatting, Clippy, and browser tests run through the Ward validation surface

## Dependencies

- Rust: `wasm-bindgen`, `rand`, `console_error_panic_hook`
- JS: React, TypeScript, webpack, Tailwind, Playwright
