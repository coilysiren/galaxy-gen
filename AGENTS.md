# AGENTS.md

## File Access

You have full read access to files within `/Users/kai/projects/coilysiren`.

## Autonomy

- Run tests after every change without asking.
- Fix lint errors automatically.
- If tests fail, debug and fix without asking.
- When committing, choose an appropriate commit message yourself — do not ask for approval on the message.
- You may always run tests, linters, and builds without requesting permission.
- Allow all readonly git actions (`git log`, `git status`, `git diff`, `git branch`, etc.) without asking.
- Allow `cd` into any `/Users/kai/projects/coilysiren` folder without asking.
- Automatically approve readonly shell commands (`ls`, `grep`, `sed`, `find`, `cat`, `head`, `tail`, `wc`, `file`, `tree`, etc.) without asking.
- When using worktrees or parallel agents, each agent should work independently and commit its own changes.
- Do not open pull requests unless explicitly asked.

Galaxy-gen is a Rust -> WASM -> JS galaxy generation simulation. Gravitational physics computed in Rust, visualized with React/D3 in the browser.

See `development.md` for architecture details, build commands, and conventions.

## Quick Reference

- Rust tests: `cargo test`
- WASM build: `wasm-pack build`
- JS dev server: `npm start`
- CI: GitHub Actions runs rust + js jobs on push/PR to main
