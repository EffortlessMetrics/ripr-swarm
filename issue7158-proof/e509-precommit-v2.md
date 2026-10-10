# ripr precommit v2

- Status: `pass`
- Failure kind: `none`
- Head: `e5094c36897599b924aecc35c8470e1538d982e7`
- Merge base: `60290d6e6dd54d24f96f79ccfa012fb5fe6dd735`
- Changed records: `51`
- Report digest: `sha256:d6930606dea5e74e35508ebb9cc688b4009c1aba62a65d8389c35073ee7bbc8e`

## Changed files

- `added` `-` -> `changelog.d/7135-collection-subject-trailing-ops.md` (committed)
- `added` `-` -> `changelog.d/7147-seam-degradation-spawn-test.md` (committed)
- `added` `-` -> `crates/ripr/tests/lsp_seam_degradation_spawn.rs` (committed)
- `added` `-` -> `tools/repo-policy/Cargo.toml` (committed)
- `added` `-` -> `tools/repo-policy/build.rs` (committed)
- `added` `-` -> `tools/repo-policy/source_identity.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/agent_skills.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/identity.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/lib.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/main.rs` (committed)
- `added` `-` -> `tools/repo-policy/src/workflow_tests.rs` (committed)
- `added` `-` -> `tools/repo-policy/tests/frontdoor.rs` (committed)
- `modified` `-` -> `.cargo/config.toml` (committed)
- `modified` `-` -> `.github/workflows/routed-rust.yml` (committed)
- `modified` `-` -> `.github/workflows/rust-gates.yml` (committed)
- `modified` `-` -> `.ripr/allow-attributes.txt` (committed)
- `modified` `-` -> `.ripr/traceability.toml` (committed)
- `modified` `-` -> `Cargo.lock` (committed)
- `modified` `-` -> `Cargo.toml` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classifier.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/propagation_witness.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/seam_cache.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/seam_inventory.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/seams.rs` (committed)
- `modified` `-` -> `crates/ripr/src/cli/help/core.rs` (committed)
- `modified` `-` -> `crates/ripr/src/output/first_pr/preflight.rs` (committed)
- `modified` `-` -> `crates/ripr/src/output/outcome/mod.rs` (committed)
- `modified` `-` -> `crates/ripr/tests/cli_help_hierarchy.rs` (committed)
- `modified` `-` -> `docs/LEARNINGS.md` (committed)
- `modified` `-` -> `docs/OUTPUT_SCHEMA.md` (committed)
- `modified` `-` -> `docs/ci/PRODUCT_GATE_PLAN.md` (committed)
- `modified` `-` -> `docs/specs/RIPR-SPEC-0005-repo-seam-inventory.md` (committed)
- `modified` `-` -> `docs/specs/RIPR-SPEC-0094-observation-unverified-guard-generalization.md` (committed)
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
- `git diff --check 60290d6e6dd54d24f96f79ccfa012fb5fe6dd735...HEAD`: `pass`
- `git diff --cached --check`: `pass`
- `git diff --check`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml --workspace --all-targets -- -D warnings`: `pass`

## Skipped

- none recorded

## Limitations

- none
