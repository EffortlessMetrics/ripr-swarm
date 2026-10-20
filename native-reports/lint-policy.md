# check-lint-policy

Status: pass

## Why This Matters

`policy/clippy-lints.toml` is the reviewable ledger of the workspace lint stance, including planned 1.94 / 1.95 flips. If Cargo.toml drifts from the ledger, reviewers lose the trajectory and the dual-rail design (clippy + semantic checker) loses its receipt. `activate_when_msrv` is compared to `[workspace.package] rust-version`; an already-met MSRV without a remaining non-MSRV `blocked_by` is overdue. `reason` is narrative and does not satisfy that gate. `policy/clippy-debt.toml` is parsed as TOML: required nonblank fields, unknown fields, duplicate keys, trailing garbage, `target` dates, and dual-rail collisions fail here rather than being trusted as comments.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-lint-policy
```
