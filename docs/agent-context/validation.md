# Validation Commands

Agents should use the smallest validation set that proves the change.

## Environment and shell

Detect the current host, shell, repository root, worktree, and toolchain before
running multi-step commands.

- On Windows, use PowerShell syntax and cmdlets. Do not assume `touch`, `sed`,
  `awk`, Bash pipelines, or POSIX exit variables exist.
- On Linux or macOS, use the active POSIX shell rather than pasting PowerShell
  syntax.
- Cargo writes normal progress to stderr. Stderr output alone is not failure;
  use the real process exit status and terminal test/build summary.
- When a wrapper obscures the exit status, rerun the command directly or
  capture the native status explicitly before classifying the result.
- Run one broad Cargo command at a time per candidate worktree. Avoid stacking
  broad compiles on the same constrained host and do not kill unrelated Cargo
  processes.

Bind background validation to its retained task/session handle. A process-name
filter, unchanged report file, missing final binary, or quiet interval does not
show that the driver died, stalled, or orphaned children. Do not call a process
harmless, abandoned, or safe to ignore without parent/driver identity, terminal
status, and cleanup evidence. When those facts are unavailable, preserve
`unknown` and do not launch a competing broad run on the same worktree/host.

A long-running command blocks only its candidate transition. Advance another
independent claim in a separate worktree or worker when useful instead of
polling unchanged process state.

## Local and remote proof authority

Local proof is for fast discrimination and candidate shaping:

```bash
cargo xtask check-fast
cargo xtask precommit
```

Add the focused tests and narrow compile required by the changed semantic
owner. Do not serially duplicate every hosted gate before publishing a coherent
candidate.

PR CI owns the required merge-gate matrix. A committed candidate may be pushed
with `REVIEW_INCOMPLETE` so hosted checks and remote artifacts can run. Green CI
does not replace substantive review, and unavailable/skipped/quota-limited
review remains missing evidence.

Release-readiness, package/install rehearsal, immutable-candidate
qualification, source integration, and publication are separate fixed-object
proof planes. A pass in one plane must not be reported as another plane's pass.

## Rust

Focused development commands vary by changed surface. The broad reference
commands are:

```bash
cargo check --workspace --all-targets
cargo test --workspace
```

Use the repository's checked test-runner contract for required CI and release
qualification. Do not assume Cargo test and nextest prove the same proposition
without the retained contract and subject inventory.

## Workflow policy

```bash
cargo xtask check-workflows
```

Run this for any change to:

- `.github/workflows/**`
- `policy/workflow_allowlist.txt`
- release workflows
- CI/security workflows

## Droid workflow policy

```bash
cargo xtask check-droid-review-config
```

Run this with `cargo xtask check-workflows` for changes to:

- `.github/workflows/droid-review.yml`
- `.github/workflows/droid.yml`
- `.github/workflows/droid-security-scan.yml`
- `docs/agent-context/review-invariants.md`
- `docs/agent-context/droid-smoke-tests.md`
- `.factory/skills/review-guidelines/SKILL.md`
- `.factory/rules/droid-review.md`

## Security-sensitive review

For changes involving secrets, workflows, dependency policy, release scripts, or command execution:

- inspect workflow permissions;
- inspect event triggers;
- inspect fork behavior;
- inspect artifact/log exposure;
- inspect whether secrets can be printed or written to repo files.
