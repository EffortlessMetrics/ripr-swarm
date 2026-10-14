# PR 7817 fixture-integrity handoff

Read-only evidence bundle for the narrowly scoped fixture-integrity repair.

## Candidate identity

- Repository: `EffortlessMetrics/ripr-swarm`
- Prior published head and parent: `a97ec757ba2bc84292729ee8675d0f5459d3808a`
- Candidate commit: `366469fdd2104d630c87da8e1c83437813101324`
- Candidate tree: `bf45b50dd767381106ed45e2ecdc2addfedb011e`
- Changed path: `crates/ripr/tests/sibling_effect_oracle.rs`
- No production analyzer paths changed.
- Toolchain: pinned Rust/Cargo/rustfmt 1.95.0, Linux x86_64, locked dependencies.

## Change

All 61 method-style `.replace(...)` calls in the changed test module now use
`replace_checked(...)`. The helper fails when the resulting string equals its
input, which catches absent patterns as well as no-op substitutions in opaque
and self-comparison controls. `fixture_replacement_rejects_missing_pattern`
intentionally supplies an absent pattern and expects the guard panic. Analyzer
expected classifications/assertions were not changed.

## Validation

- `cargo test --locked -p ripr --test sibling_effect_oracle`: exit 0; 15 passed.
- Focused control `cargo test --locked -p ripr --test sibling_effect_oracle fixture_replacement_rejects_missing_pattern -- --exact --nocapture`: exit 0; captured panic says the absent pattern matched no source text; `should panic` passes.
- `cargo fmt --check --all -- --check`: exit 0 on the candidate change.
- `cargo xtask precommit`: exit 0, including strict workspace/all-target Clippy. Its release-challenge selection report retains an existing unmet authorized-repository floor (`0 < 2`, read-only scope); it was reported and not lowered. The precommit command itself passed.
- `git diff --check`: exit 0.
- No full corpus, hosted qualification, Ready state, or merge qualification is claimed. The separate #7880 external corpus setup blocker is unchanged.

Raw command output is retained in `format.log`, `focused-test.log`,
`no-op-control.log`, and `precommit.log`. `independent-review.md` records the
frozen-tree review and its runtime-test limitation. `push.log` records the
fast-forward publish result. Review comments 4266778104 and 4268030116 remain
unresolved as requested. The published logs retain command output while
normalizing the disposable candidate checkout path to `<candidate-worktree>`;
the original local receipts remain at `/workspace/ripr-fixture-evidence`.
