# ripr precommit v2

- Status: `pass`
- Failure kind: `none`
- Head: `8bbc8dd4dead78cdccdb10861fe4848103aac6dd`
- Merge base: `67866bc95a72462840c3eedac0f213ed658474b4`
- Changed records: `115`
- Report digest: `sha256:d55ce1c91542801addebaed70573a0b23048c83111917f6d3a976a36a261f873`

- Composition: `local_precommit`

## Changed files

- `added` `-` -> `changelog.d/7415-inherent-receiver-admission.md` (committed)
- `added` `-` -> `crates/ripr/tests/private_inherent_receiver.rs` (committed)
- `added` `-` -> `fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-receiver-dormant-body.json` (committed)
- `added` `-` -> `fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-receiver-foreign-helper.json` (committed)
- `added` `-` -> `fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-receiver-free-helper-identity.json` (committed)
- `added` `-` -> `fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-receiver-helper-scope-shadow.json` (committed)
- `added` `-` -> `fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-receiver-helper-type-parameter.json` (committed)
- `added` `-` -> `fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-receiver-local-wrapper.json` (committed)
- `added` `-` -> `fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-receiver-macro-discard.json` (committed)
- `added` `-` -> `fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-receiver-module-type-shadow.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/SPEC.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/controls/dormant-async.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/controls/dormant-closure.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/controls/dormant-plain.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/controls/nested-invoked.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/controls/outer-call.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/diff.patch` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/expected/check.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/expected/human.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/inherent_receiver_dormant_body/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/SPEC.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/controls/local-helper.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/controls/shadowed-owner-type.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/diff.patch` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/expected/check.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/expected/human-full.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/expected/human.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/inherent_receiver_foreign_helper/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/BOUNDARIES.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/SPEC.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/cases.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/generic-root.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/genuine-root.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/imported-foreign.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/imported-root.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/intermediate-foreign.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/intermediate-root.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/qualified-test-module-helper.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/same-line-callers.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/same-line-foreign.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/test-module-helper.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/controls/unique-foreign-value.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/diff.patch` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/expected/check.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/expected/human.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/inherent_receiver_free_helper_identity/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_helper_scope_shadow/SPEC.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_helper_scope_shadow/controls/genuine-module.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_helper_scope_shadow/controls/receiver-type-parameter.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_helper_scope_shadow/diff.patch` (committed)
- `added` `-` -> `fixtures/inherent_receiver_helper_scope_shadow/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_helper_scope_shadow/expected/check.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_helper_scope_shadow/expected/human.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_helper_scope_shadow/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/inherent_receiver_helper_scope_shadow/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/SPEC.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/controls/foreign-entry.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/controls/foreign-last-hop.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/diff.patch` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/expected/check.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/expected/human-full.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/expected/human.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/inherent_receiver_local_wrapper/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/SPEC.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/controls/sibling-shadow.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/controls/standard.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/diff.patch` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/expected/check.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/expected/human-full.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/expected/human.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/inherent_receiver_macro_discard/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_module_type_shadow/SPEC.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_module_type_shadow/controls/genuine-owner.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_module_type_shadow/diff.patch` (committed)
- `added` `-` -> `fixtures/inherent_receiver_module_type_shadow/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_module_type_shadow/expected/check.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_module_type_shadow/expected/human.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_module_type_shadow/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/inherent_receiver_module_type_shadow/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/inherent_receiver_operand_item_shadow/SPEC.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_operand_item_shadow/diff.patch` (committed)
- `added` `-` -> `fixtures/inherent_receiver_operand_item_shadow/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/inherent_receiver_operand_item_shadow/expected/check.json` (committed)
- `added` `-` -> `fixtures/inherent_receiver_operand_item_shadow/expected/human-full.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_operand_item_shadow/expected/human.txt` (committed)
- `added` `-` -> `fixtures/inherent_receiver_operand_item_shadow/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/inherent_receiver_operand_item_shadow/input/src/lib.rs` (committed)
- `modified` `-` -> `.ripr/traceability.toml` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/call_identity.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/mod.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/owner_pin.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/related_tests.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/syntax/mod.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/syntax/name_scope.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/test_grip_evidence/related_tests.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/test_grip_evidence/related_tests/context.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/test_grip_evidence/tests.rs` (committed)
- `modified` `-` -> `docs/LEARNINGS.md` (committed)
- `modified` `-` -> `docs/specs/RIPR-SPEC-0005-repo-seam-inventory.md` (committed)
- `modified` `-` -> `fixtures/evidence-promotion-honesty-corpus/corpus.json` (committed)
- `modified` `-` -> `fixtures/rust-verdict-corpus/expected/rows/bytesize-as-kib-div.json` (committed)
- `modified` `-` -> `fixtures/rust-verdict-corpus/expected/rows/bytesize-as-mb-div.json` (committed)
- `modified` `-` -> `fixtures/rust-verdict-corpus/expected/rows/lib-rstest-fixture-injected-receiver.json` (committed)
- `modified` `-` -> `fixtures/rust-verdict-corpus/expected/rows/serde-derive-rename-field-upper.json` (committed)
- `modified` `-` -> `fixtures/rust-verdict-corpus/expected/rows/serde-derive-rename-variant-lower.json` (committed)

## Impact plan

- package: `ripr`
- package: `xtask`

## Commands

- `cargo metadata --format-version 1 --manifest-path $REPO/Cargo.toml`: `pass`
- `existing repository policy precommit`: `pass`
- `git diff --check 67866bc95a72462840c3eedac0f213ed658474b4...HEAD`: `pass`
- `git diff --cached --check`: `pass`
- `git diff --check`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml -p ripr --all-targets -- -D warnings`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml -p xtask --all-targets -- -D warnings`: `pass`

## Child command diagnostics

Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining; enforced timeout duration includes termination/reaping and Unix process-group confirmation where applicable. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate.

Capture scope: diff checks and selected Clippy children only. Metadata, aggregate policy gates and their children have unavailable diagnostics here. Argv retains argument boundaries with repository-root substitution `$REPO`. HEAD/base and changed paths are context, not an immutable digest of dirty source.

- program="git"; argv=["-C", "$REPO", "diff", "--check", "67866bc95a72462840c3eedac0f213ed658474b4...HEAD"]; outcome=pass; status=exit status: 0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--cached", "--check"]; outcome=pass; status=exit status: 0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--check"]; outcome=pass; status=exit status: 0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="cargo"; argv=["clippy", "--manifest-path", "$REPO/Cargo.toml", "-p", "ripr", "--all-targets", "--", "-D", "warnings"]; outcome=pass; status=exit status: 0; elapsed_ms=304; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="cargo"; argv=["clippy", "--manifest-path", "$REPO/Cargo.toml", "-p", "xtask", "--all-targets", "--", "-D", "warnings"]; outcome=pass; status=exit status: 0; elapsed_ms=142095; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=recognized_stderr_lines(compiling=unavailable, checking=6, fresh=unavailable)

## Skipped

- none recorded

## Limitations

- none
