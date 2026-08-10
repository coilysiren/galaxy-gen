# Ward config

What the blocks in [`.ward/ward.yaml`](../.ward/ward.yaml) mean. The file itself
carries a three-line pointer here, because YAML comments are only legal above
the first content line and long prose belongs in docs.

## commands

Ward is the canonical entry point for dev verbs. Agents run `ward exec <verb>`,
never the bare tool, and the lockdown denies direct `cargo`, `wasm-pack`, and
`npx` invocations. Add a verb here before invoking it.

Verb names follow ward's `[a-z0-9-]` rule. Argv validation rejects shell
metacharacters at invocation time, so flags forward verbatim through ward's argv
handling. Multi-step workflows live in one focused helper,
`scripts/ward-command.sh`, rather than being spelled out inline.

## capabilities

The provider skills this leaf pulls into its git-excluded `.agents/skills/` for
confined harnesses (OpenClaw on Qwen). `scripts/pull-capabilities.py` resolves
them and `scripts/validate-harness-skills.py` enforces the result. The spec is
the `capability_pull` block of agentic-os-kai `.agents/skills/categories.yaml`.

Keep the list minimal. Every entry widens what a 25k-budget Qwen session has to
read before it starts. galaxy-gen is a Rust to WASM core plus a React, D3, and
TypeScript browser surface, so the list is exactly those two languages.

Infra skills (k8s, terraform) are deliberately absent. Qwen escalates infra
rather than running it, per `kai-qwen-scope`.

## agent

`workflow` is the landing lane a dispatched agent follows. See the Workflow
section of [AGENTS.md](../AGENTS.md) for what this repo's setting means in
practice.

## catalog

Metadata for the cross-repo knowledge graph. The schema is tracked in
agentic-os-kai#420.

## security

`protected_binaries` and the `sudo` block bound what a ward session may do to
the host. Both stay conservative here: this repo builds a static site and needs
no privileged operations.

## See also

- [AGENTS.md](../AGENTS.md) - agent operating context, including the landing workflow.
- [development.md](../development.md) - architecture and the dev loop these verbs drive.
