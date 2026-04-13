# AGENTS.md

Galaxy-gen is a Rust -> WASM -> JS galaxy generation simulation. Gravitational physics computed in Rust, visualized with React/D3 in the browser.

See `development.md` for architecture details, build commands, and conventions.

## Quick Reference

- Rust tests: `cargo test`
- WASM build: `wasm-pack build`
- JS dev server: `npm start`
- CI: GitHub Actions runs rust + js jobs on push/PR to main
