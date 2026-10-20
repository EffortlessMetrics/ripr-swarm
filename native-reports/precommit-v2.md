# ripr precommit v2

- Status: `pass`
- Failure kind: `none`
- Head: `fc7e17a409cf81b99c2447a409b55ec699752d06`
- Merge base: `1e9468f4b0d133f0a50b64134318763d8cf48beb`
- Changed records: `13`
- Report digest: `sha256:9c36394e4eefcbdda4647dc82723af171902a60e60c89259bc9856ef5ee7dfce`

- Composition: `local_precommit`

## Changed files

- `added` `-` -> `changelog.d/brief-parser-refusal-status.md` (committed)
- `added` `-` -> `crates/ripr/src/analysis/diff/parse/stream/path_admission_tests.rs` (committed)
- `added` `-` -> `crates/ripr/tests/task_first_facade/brief_diff_status.rs` (committed)
- `modified` `-` -> `.ripr/traceability.toml` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/diff/parse.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/diff/parse/stream.rs` (committed)
- `modified` `-` -> `crates/ripr/src/analysis/mod.rs` (committed)
- `modified` `-` -> `crates/ripr/src/cli/commands_agent_support.rs` (committed)
- `modified` `-` -> `crates/ripr/src/core_error.rs` (committed)
- `modified` `-` -> `crates/ripr/tests/task_first_facade.rs` (committed)
- `modified` `-` -> `docs/OUTPUT_SCHEMA.md` (committed)
- `modified` `-` -> `docs/specs/RIPR-SPEC-0010-agent-working-set-brief.md` (committed)
- `modified` `-` -> `docs/specs/RIPR-SPEC-0241-check-json-refusal-envelope.md` (committed)

## Impact plan

- package: `ripr`
- package: `xtask`

## Commands

- `cargo metadata --format-version 1 --no-deps --manifest-path $REPO/Cargo.toml`: `pass`
- `existing repository policy precommit`: `pass`
- `git diff --check 1e9468f4b0d133f0a50b64134318763d8cf48beb...HEAD`: `pass`
- `git diff --cached --check`: `pass`
- `git diff --check`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml -p ripr --all-targets -- -D warnings`: `pass`
- `cargo clippy --manifest-path $REPO/Cargo.toml -p xtask --all-targets -- -D warnings`: `pass`

## Child command diagnostics

Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining; enforced timeout duration includes termination/reaping and Unix process-group confirmation where applicable. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate.

Capture scope: diff checks and selected Clippy children only. Metadata, aggregate policy gates and their children have unavailable diagnostics here. Argv retains argument boundaries with repository-root substitution `$REPO`. HEAD/base and changed paths are context, not an immutable digest of dirty source.

- program="git"; argv=["-C", "$REPO", "diff", "--check", "1e9468f4b0d133f0a50b64134318763d8cf48beb...HEAD"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=101; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--cached", "--check"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=101; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--check"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="cargo"; argv=["clippy", "--manifest-path", "$REPO/Cargo.toml", "-p", "ripr", "--all-targets", "--", "-D", "warnings"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=802; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="cargo"; argv=["clippy", "--manifest-path", "$REPO/Cargo.toml", "-p", "xtask", "--all-targets", "--", "-D", "warnings"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=159331; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=recognized_stderr_lines(compiling=unavailable, checking=2, fresh=unavailable)

## Skipped

- none recorded

## Limitations

- none
