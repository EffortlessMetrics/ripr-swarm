# ripr precommit v2

- Status: `pass`
- Failure kind: `none`
- Head: `21fada8b88428a0ea90216e590dd3304fcbec1f1`
- Merge base: `331176f6cf6405ba9688b7a5dbca6654453eaad1`
- Changed records: `36`
- Report digest: `sha256:126067c2ac3ab7e75eafd9990735b1b8df79879e7f5096eb06d5818c46cc10ae`

## Changed files

- `added` `-` -> `tools/repo-policy/Cargo.toml` (committed)
- `added` `-` -> `tools/repo-policy/build.rs` (committed)
- `added` `-` -> `tools/repo-policy/source_identity.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/agent_skills.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/identity.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/lib.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/main.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/workflow_tests.rs` (committed)
- `added` `-` -> `tools/repo-policy/tests/frontdoor.rs` (committed)
- `modified` `-` -> `.agents/skills/build-candidate/SKILL.md` (committed)
- `modified` `-` -> `.cargo/config.toml` (committed)
- `modified` `-` -> `.github/workflows/routed-rust.yml` (committed)
- `modified` `-` -> `.github/workflows/rust-gates.yml` (committed)
- `modified` `-` -> `.ripr/allow-attributes.txt` (committed)
- `modified` `-` -> `Cargo.lock` (committed)
- `modified` `-` -> `Cargo.toml` (committed)
- `modified` `-` -> `docs/LEARNINGS.md` (committed)
- `modified` `-` -> `docs/ci/PRODUCT_GATE_PLAN.md` (committed)
- `modified` `-` -> `policy/dependency_allowlist.txt` (committed)
- `modified` `-` -> `policy/non-rust-allowlist.toml` (committed)
- `modified` `-` -> `policy/process_allowlist.txt` (committed)
- `modified` `-` -> `policy/workflow_allowlist.txt` (committed)
- `modified` `-` -> `policy/workspace_shape.txt` (committed)
- `modified` `-` -> `xtask/Cargo.toml` (committed)
- `modified` `-` -> `xtask/src/agent_skills.rs` (committed)
- `modified` `-` -> `xtask/src/check_fast_strict.rs` (committed)
- `modified` `-` -> `xtask/src/dogfood.rs` (committed)
- `modified` `-` -> `xtask/src/fixture_contracts/report_validators.rs` (committed)
- `modified` `-` -> `xtask/src/main.rs` (committed)
- `modified` `-` -> `xtask/src/policy/mod.rs` (committed)
- `modified` `-` -> `xtask/src/product_gate_plan.rs` (committed)
- `modified` `-` -> `xtask/src/tests.rs` (committed)
- `modified` `-` -> `xtask/src/types.rs` (committed)
- `modified` `-` -> `xtask/tests/rust_gate_workflow_contract.rs` (committed)
- `renamed` `xtask/src/policy/ci_scratch.rs` -> `tools/repo-policy/src/ci_scratch.rs` (committed)
- `renamed` `xtask/src/policy/ci_scratch/tests.rs` -> `tools/repo-policy/src/ci_scratch/tests.rs` (committed)

## Impact plan

- workspace Clippy: `true`
- reason: workspace-wide Rust policy changed: .cargo/config.toml

## Commands

- `cargo metadata --format-version 1 --manifest-path $REPO/Cargo.toml`: `pass`
- `existing repository policy precommit`: `pass`
- `git diff --check 331176f6cf6405ba9688b7a5dbca6654453eaad1...HEAD`: `pass`
- `git diff --cached --check`: `pass`
- `git diff --check`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml --workspace --all-targets -- -D warnings`: `pass`

## Skipped

- none recorded

## Limitations

- none
