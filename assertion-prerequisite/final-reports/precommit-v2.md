# ripr precommit v2

- Status: `pass`
- Failure kind: `none`
- Head: `c4a8f7828e15426c4d872718f66bd4bac3ab0daf`
- Merge base: `ddfa2a138ae816cdd46e639c532900d2abe60b27`
- Changed records: `81`
- Report digest: `sha256:63329147c544d44b2b74a12779b874472a4e3fcb2752652216d8970907053da4`

- Composition: `local_precommit`

## Changed files

- `added` `-` -> `changelog.d/5500-helper-mutable-field-selector.md` (committed)
- `added` `-` -> `changelog.d/5500-local-module-glob-assertion-identity.md` (committed)
- `added` `-` -> `changelog.d/5500-whole-tail-match-observation.md` (committed)
- `added` `-` -> `crates/ripr/src/analysis/classify/owner_pin/whole_tail_match.rs` (committed)
- `added` `-` -> `crates/ripr/src/analysis/value_resolution/option_field.rs` (committed)
- `added` `-` -> `crates/ripr/tests/local_module_glob_assertions.rs` (committed)
- `added` `-` -> `crates/ripr/tests/whole_tail_match_observation.rs` (committed)
- `added` `-` -> `fixtures/field_match_helper_mutable/SPEC.md` (committed)
- `added` `-` -> `fixtures/field_match_helper_mutable/diff.patch` (committed)
- `added` `-` -> `fixtures/field_match_helper_mutable/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/field_match_helper_mutable/expected/check.json` (committed)
- `added` `-` -> `fixtures/field_match_helper_mutable/expected/human-full.txt` (committed)
- `added` `-` -> `fixtures/field_match_helper_mutable/expected/human.txt` (committed)
- `added` `-` -> `fixtures/field_match_helper_mutable/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/field_match_helper_mutable/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/field_match_helper_taken/SPEC.md` (committed)
- `added` `-` -> `fixtures/field_match_helper_taken/diff.patch` (committed)
- `added` `-` -> `fixtures/field_match_helper_taken/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/field_match_helper_taken/expected/check.json` (committed)
- `added` `-` -> `fixtures/field_match_helper_taken/expected/human-full.txt` (committed)
- `added` `-` -> `fixtures/field_match_helper_taken/expected/human.txt` (committed)
- `added` `-` -> `fixtures/field_match_helper_taken/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/field_match_helper_taken/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/SPEC.md` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/diff.patch` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/expected/check.json` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/expected/human.txt` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/input/src/foreign.rs` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/proc-control/Cargo.toml` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_expected/proc-control/src/lib.rs` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/SPEC.md` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/ancestor-control/opaque.rs` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/ancestor-control/src/lib.rs` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/ancestor-control/src/owned.rs` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/diff.patch` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/expected/check.json` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/expected/human.txt` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/input/src/foreign.rs` (committed)
- `added` `-` -> `fixtures/whole_tail_match_macro_scalar/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/whole_tail_match_scrutinee_return/SPEC.md` (committed)
- `added` `-` -> `fixtures/whole_tail_match_scrutinee_return/diff.patch` (committed)
- `added` `-` -> `fixtures/whole_tail_match_scrutinee_return/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/whole_tail_match_scrutinee_return/expected/check.json` (committed)
- `added` `-` -> `fixtures/whole_tail_match_scrutinee_return/expected/human.txt` (committed)
- `added` `-` -> `fixtures/whole_tail_match_scrutinee_return/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/whole_tail_match_scrutinee_return/input/src/lib.rs` (committed)
- `modified` `-` -> `.ripr/traceability.toml` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classifier.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classifier/evidence.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/arm_selection.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/arm_selection/field_selector.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/context.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/mod.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/owner_pin.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/owner_pin/tests.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/related_tests.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/reveal.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/language/rust/mod.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/probes/propagation_boundary_tests.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/seam_cache.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/syntax/mod.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/syntax/module_tree.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/syntax/owner_pin.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/value_resolution.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/value_resolution/tests.rs` (committed)
- `modified` `-` -> `crates/ripr/tests/field_match_selector.rs` (committed)
- `modified` `-` -> `crates/ripr/tests/lsp_lifecycle/gap_corpus_currentness.rs` (committed)
- `modified` `-` -> `docs/OUTPUT_SCHEMA.md` (committed)
- `modified` `-` -> `docs/specs/RIPR-SPEC-0197-owner-return-pins.md` (committed)
- `modified` `-` -> `docs/specs/RIPR-SPEC-0229-match-arm-selection-discriminator.md` (committed)
- `modified` `-` -> `fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt/after.repo-exposure.json` (committed)
- `modified` `-` -> `fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt/agent-receipt.json` (committed)
- `modified` `-` -> `fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt/agent-verify.json` (committed)
- `modified` `-` -> `fixtures/boundary_gap/expected/first-useful-action/unchanged-after-attempt/before.repo-exposure.json` (committed)
- `modified` `-` -> `fixtures/evidence-promotion-honesty-corpus/corpus.json` (committed)
- `modified` `-` -> `xtask/src/tests/rust_corpus_projection.rs` (committed)

## Impact plan

- package: `ripr`
- package: `xtask`

## Commands

- `cargo metadata --format-version 1 --no-deps --manifest-path $REPO/Cargo.toml`: `pass`
- `existing repository policy precommit`: `pass`
- `git diff --check ddfa2a138ae816cdd46e639c532900d2abe60b27...HEAD`: `pass`
- `git diff --cached --check`: `pass`
- `git diff --check`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml -p ripr --all-targets -- -D warnings`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml -p xtask --all-targets -- -D warnings`: `pass`

## Child command diagnostics

Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining; enforced timeout duration includes termination/reaping and Unix process-group confirmation where applicable. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate.

Capture scope: diff checks and selected Clippy children only. Metadata, aggregate policy gates and their children have unavailable diagnostics here. Argv retains argument boundaries with repository-root substitution `$REPO`. HEAD/base and changed paths are context, not an immutable digest of dirty source.

- program="git"; argv=["-C", "$REPO", "diff", "--check", "ddfa2a138ae816cdd46e639c532900d2abe60b27...HEAD"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--cached", "--check"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--check"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="cargo"; argv=["clippy", "--manifest-path", "$REPO/Cargo.toml", "-p", "ripr", "--all-targets", "--", "-D", "warnings"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=401; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="cargo"; argv=["clippy", "--manifest-path", "$REPO/Cargo.toml", "-p", "xtask", "--all-targets", "--", "-D", "warnings"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=143707; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=recognized_stderr_lines(compiling=unavailable, checking=3, fresh=unavailable)

## Skipped

- none recorded

## Limitations

- none
