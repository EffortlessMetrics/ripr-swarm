# ripr precommit v2

- Status: `pass`
- Failure kind: `none`
- Head: `e98ec21bb82dab8d56619625d190998290f0f4b7`
- Merge base: `67866bc95a72462840c3eedac0f213ed658474b4`
- Changed records: `4`
- Report digest: `sha256:d489bc845044258ff5ba4c4a2792630bec58ee9853f4b64b49666191e68574e4`

- Composition: `ci_policy`

## Changed files

- `added` `-` -> `changelog.d/7903-advisory-evidence-report.md` (committed)
- `added` `-` -> `xtask/examples/ub_review_evidence_report.rs` (committed)
- `modified` `-` -> `.github/workflows/ub-review.yml` (committed)
- `modified` `-` -> `policy/workflow_allowlist.txt` (committed)

## Impact plan

- package: `xtask`

## Commands

- `cargo metadata --format-version 1 --manifest-path $REPO/Cargo.toml`: `pass`
- `existing repository policy precommit`: `pass`
- `git diff --check 67866bc95a72462840c3eedac0f213ed658474b4...HEAD`: `pass`
- `git diff --cached --check`: `pass`
- `git diff --check`: `pass`

## Child command diagnostics

Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining; enforced timeout duration includes termination/reaping and Unix process-group confirmation where applicable. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate.

Capture scope: diff checks and selected Clippy children only. Metadata, aggregate policy gates and their children have unavailable diagnostics here. Argv retains argument boundaries with repository-root substitution `$REPO`. HEAD/base and changed paths are context, not an immutable digest of dirty source.

- program="git"; argv=["-C", "$REPO", "diff", "--check", "67866bc95a72462840c3eedac0f213ed658474b4...HEAD"]; outcome=pass; status=exit status: 0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--cached", "--check"]; outcome=pass; status=exit status: 0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable
- program="git"; argv=["-C", "$REPO", "diff", "--check"]; outcome=pass; status=exit status: 0; elapsed_ms=100; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=unavailable

## Skipped

- local selected Clippy is outside ci-policy; CI must run its explicit cargo clippy --workspace --all-targets -- -D warnings obligation

## Limitations

- none
