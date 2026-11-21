# GitHub Copilot instructions

Use the root [AGENTS.md](../AGENTS.md) as the RIPR operating contract and
[.agents/skills](../.agents/skills) for repository procedures. Treat
`CLAUDE.md` and `.claude/` as reference material for Claude configuration work.
Reserve `AGENTS.override.md` for Codex.

## Prepare a GitHub Actions environment

For explicit online preparation of a supported Linux image, run the shared
[cloud install recipe](../CONTRIBUTING.md#cloud-install-and-restored-task-recipes)
from the checkout root. It installs the checkout's pinned Rust SDK and
refreshes stable for separate qualification.

```bash
node tools/provision-dev.cjs --install-system-deps
```

## Start work on the checkout

In the Bash shell that will run Cargo, activate the installed compiler
selected by this checkout's `rust-toolchain.toml`.

```bash
source tools/activate-rust.sh --pinned &&
mkdir -p target
```

A started session alone does not establish setup readiness. If a setup
receipt is missing, stale, or reports a failure, run the read-only
`node editors/vscode/test/developerSetup.cjs --json` before starting broad
work. Use the current checkout's setup evidence and any applicable recovery
command from a failed receipt.
