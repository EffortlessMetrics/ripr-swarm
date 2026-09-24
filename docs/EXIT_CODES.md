# Exit Codes

ripr uses a simple two-value exit code contract:

| Code | Meaning |
|------|---------|
| `0`  | The command completed. |
| `2`  | The command could not complete, or a command that owns a blocking decision blocked. |

`ripr check` is advisory: it exits `0` whether or not it found gaps, including
`exposed` and `weakly_exposed` findings. A non-zero exit from `check` means the
analysis did not complete, not that it disapproved of your diff. The blocking
decision belongs to `ripr gate evaluate`, which exits `2` when the gate blocks.
Do not build a CI gate on `check`'s exit code; read its output, or run the gate.

## Why code 2?

ripr uses exit code `2` for all failure conditions (not `1`) to distinguish
it from shell-level errors (which typically use `1`). This makes it easier
to distinguish "ripr ran and found issues" from "the shell could not run
ripr" in CI pipelines.

## When you see exit code 2

- **Gate failure**: `ripr gate evaluate` blocked the PR. This is the only
  findings-driven exit code ripr emits.
- **Analysis error**: the diff could not be parsed, the base ref could not
  be resolved, or the workspace root could not be determined.
- **User error**: unknown command, missing required argument, or invalid
  config.
- **Internal error**: a panic occurred (with a `ripr: internal error` message).

## `ripr doctor` exit codes

`ripr doctor` uses the same contract: `0` when all checks pass, `2` when
any check fails (including missing language runtimes that are enabled in
the effective configuration).

The `Cargo.toml`, `cargo`, and `rustc` checks apply only when Rust is in
scope for the root: Rust is enabled and either Rust markers (`Cargo.toml` or
`.rs` files) are detected or no other language is detected or enabled. A
Python-only or TypeScript-only root reports those checks as `skipped` with the
reason and does not fail on them. A Rust root, a root with Rust sources but no
`Cargo.toml`, and an empty root under the Rust-only default still fail on a
missing manifest or toolchain.

## CI integration

In generated GitHub Actions workflows, ripr preserves the exit code:

```yaml
ripr check --root . --mode draft --format json > check.json || check_status=$?
```

The `|| check_status=$?` pattern captures the exit code without failing
the step, so downstream review-comments and gate steps can consume the
output even when the analysis failed. Because `check` exits `0` on findings,
a non-zero `check_status` here means the analysis itself did not complete.
