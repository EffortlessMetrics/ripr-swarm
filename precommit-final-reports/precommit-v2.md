# ripr precommit v2

- Status: `pass`
- Failure kind: `none`
- Head: `9fc6a47f29b5ad581a527244681d4c4bf0d4714b`
- Merge base: `006eaf853fce5713a2e9044ba9c8d6a9a873f3d0`
- Changed records: `18`
- Report digest: `sha256:dc70d640fe103a9eb99da34430a516e9634335a802129e19aac9faef358ad111`

- Composition: `local_precommit`

## Changed files

- `added` `-` -> `changelog.d/8265-macro-site-parity.md` (committed)
- `added` `-` -> `crates/ripr/src/analysis/classify/owner_pin/tests/retention_parity.rs` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/SPEC.md` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/diff.patch` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/expected/CHANGELOG.md` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/expected/check.json` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/expected/human-full.txt` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/expected/human.txt` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/input/Cargo.toml` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/input/src/globs.rs` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/input/src/lib.rs` (committed)
- `added` `-` -> `fixtures/macro_site_scoped_rest_limit/input/tests/buf_tests.rs` (committed)
- `modified` `-` -> `.ripr/traceability.toml` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/owner_pin.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/classify/owner_pin/tests.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/syntax/mod.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/syntax/owner_pin.rs` (committed)
- `modified` `-` -> `fixtures/evidence-promotion-honesty-corpus/corpus.json` (committed)

## Impact plan

- package: `ripr`
- package: `xtask`

## Commands

- `cargo metadata --format-version 1 --no-deps --manifest-path $REPO/Cargo.toml`: `pass`
- `existing repository policy precommit`: `pass`
- `git diff --check 006eaf853fce5713a2e9044ba9c8d6a9a873f3d0...HEAD`: `pass`
- `git diff --cached --check`: `pass`
- `git diff --check`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml -p ripr --all-targets -- -D warnings`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml -p xtask --all-targets -- -D warnings`: `pass`

## Child command diagnostics

Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining; enforced timeout duration includes termination/reaping and Unix process-group confirmation where applicable. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate.

Capture scope: diff checks and selected Clippy children only. Metadata, aggregate policy gates and their children have unavailable diagnostics here. Argv retains argument boundaries with repository-root substitution `$REPO`. HEAD/base and changed paths are context, not an immutable digest of dirty source.

- program="git"; argv=["-C", "$REPO", "diff", "--check", "006eaf853fce5713a2e9044ba9c8d6a9a873f3d0...HEAD"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--cached", "--check"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--check"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="cargo"; argv=["clippy", "--manifest-path", "$REPO/Cargo.toml", "-p", "ripr", "--all-targets", "--", "-D", "warnings"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=132115; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=recognized_stderr_lines(compiling=1, checking=unavailable, fresh=unavailable)
- program="cargo"; argv=["clippy", "--manifest-path", "$REPO/Cargo.toml", "-p", "xtask", "--all-targets", "--", "-D", "warnings"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=137278; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=recognized_stderr_lines(compiling=7, checking=22, fresh=unavailable)

## Skipped

- none recorded

## Limitations

- none
