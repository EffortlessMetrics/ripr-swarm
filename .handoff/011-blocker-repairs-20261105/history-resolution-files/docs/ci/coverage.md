# Coverage Reports

Coverage reporting is **execution-surface evidence only**. It shows whether changed code paths executed during testing, but does not prove correctness, conformance, safety, completeness, or mutation adequacy.

## What coverage measures

`ripr`'s coverage reports use `cargo-llvm-cov` to generate line and branch coverage over:
- Product code: `crates/ripr/src/`
- Automation code: `xtask/src/`

Coverage artifacts are excluded for:
- `target/` (build output)
- `fixtures/**/target/` (fixture build output)
- `editors/vscode/**/node_modules/` (extension dependencies)
- `xtask/src/reports/release.rs` (release automation)

## Claim boundaries

The Codecov badge and coverage reports do **not** prove:
- Test discriminator adequacy (coverage does not equal mutation killing)
- Seam classification completeness
- Oracle strength across all five RIPR stages
- Static analysis correctness (dynamic mutation testing required)
- Reproducibility across different versions of `cargo-llvm-cov`

Coverage is **advisory** and does not block merges. Codecov status checks are informational by default.

## Baseline targets

As of 2026-05-07, the project coverage baseline is 75.5%:
- **Product code** (`crates/ripr/src/`): 94.8% coverage; target 94% (project), 94% (patch)
- **Automation** (`xtask/src/`): 59% coverage; target 59% (project), 75% (patch)

Thresholds:
- **Project**: 1% for product, 1% for automation
- **Patch**: 3% for product, 10% for automation

## Manual verification

Use `workflow_dispatch` on the `Coverage` workflow to verify after changing:
- `.github/workflows/coverage.yml`
- `codecov.yml`
- Coverage-related policy entries
- Test topology that may affect `cargo-llvm-cov`

Expected artifacts after a coverage run:
- `lcov.info`, uploaded by GitHub Actions as the `rust-lcov` artifact.
- `target/ripr/reports/coverage-diagnostics/`, uploaded as
  `coverage-failure-diagnostics` only when coverage generation fails, including
  failures before profile merging. This bounded
  artifact records profile-list inventory, validated replay status, limited
  malformed-profile prefixes, and tool diagnostics; it does not change the
  failed coverage result. Tool-stream UTF-8 validity flags describe only the
  retained bounded prefix; bytes beyond that prefix are discarded. Exact
  retained bytes drive warning-path matching, while display text is sanitized.
  The required Linux LLVM replay test fails if Rust or matching LLVM discovery
  fails; unsupported platforms exercise the adapter refusal control.

The failure manifest accepts LLVM raw64 little-endian profile versions 10 and
11 for binary-ID extraction. Version 10 has a 128-byte header; version 11 has a
152-byte header. The ID block starts after that version's complete header, and
available or partial ID results report `llvm_raw64_le_v10` or
`llvm_raw64_le_v11`. Other versions, variant flags and magic values remain
explicitly unsupported; accepting an ID does not validate the rest of a profile.

Replaying an older profile with newer LLVM can report a raw profile version
mismatch. The manifest retains that warned profile's bounded prefix as
`raw_profile_version_mismatch` only when the warning names an exact validated
snapshot input. Unknown warnings and unrelated paths remain unclassified.
This retains diagnostic evidence without converting the profile or changing
the failed coverage result.

The failure manifest also associates supported raw-profile binary IDs with GNU
build IDs in instrumented Linux ELF files found only as direct entries in the
coverage target's `debug`, `debug/deps`, `release`, and `release/deps`
directories. It captures bounded
identifiers and file metadata, never whole binaries. Exact ID equality and LLVM
profile sections are required; distinct instrumented candidates with the same
ID remain ambiguous, while duplicate directory entries of one executable
(cargo's hard-linked `debug/<name>` copy of `debug/deps/<name>-<hash>`) collapse
to a single candidate by file identity.
Unavailable, unmatched within the inspected scope, and truncated results remain
explicit. Inspection is limited to 1,024 directory entries, 256 files, 2 MiB of
reads (64 KiB per file), 16 matches, 32 KiB of mapping JSON, and two seconds.
The profile parser retains the valid prefix when a later ID exceeds the 32-ID
limit, reports that profile as partial, and marks the aggregate mapping partial
so unmatched retained IDs are not treated as definitive misses. An unavailable
profile-ID input also keeps unmatched IDs inconclusive. A readable,
structurally parsed executable without LLVM profile sections is counted as a
noncandidate; malformed, unsupported, or unreadable inputs remain unavailable
and can make the scope partial. Malformed/unsupported layouts and mapping
failures do not discard profiles or change the original failed coverage
result. A build-ID association does not
identify an owning test, process termination, or the cause of a short profile.

Codecov upload requires the `CODECOV_TOKEN` secret and runs only for trusted
pushes, manual dispatches, and same-repo pull requests. Fork pull requests still
generate `lcov.info` and upload the `rust-lcov` artifact, but skip Codecov
upload because repository secrets are unavailable.

### Validate locally

On Linux, before adopting a newer Rust toolchain, run the small diagnostics package with
both the repository pin and stable. Install matching LLVM tools for each
selected compiler; a system `llvm-profdata` from another LLVM release is not a
substitute for the tool in that compiler's sysroot.

```bash
rustup component add --toolchain 1.95.0 clippy llvm-tools-preview
rustup update stable
rustup component add --toolchain stable clippy llvm-tools-preview
rustc +stable -vV
ripr_stable_sysroot="$(rustc +stable --print sysroot)"
ripr_stable_host="$(rustc +stable -vV | sed -n 's/^host: //p')"
"$ripr_stable_sysroot/lib/rustlib/$ripr_stable_host/bin/llvm-profdata" --version
RUSTUP_TOOLCHAIN=1.95.0 cargo test --locked -p repo-policy --lib coverage_diagnostics
RUSTUP_TOOLCHAIN=stable cargo test --locked -p repo-policy --lib coverage_diagnostics -- --nocapture
RUSTUP_TOOLCHAIN=stable cargo test --locked -p repo-policy --test frontdoor
RUSTUP_TOOLCHAIN=stable cargo clippy --locked -p repo-policy --all-targets -- -D warnings
```

`rustup update stable` refreshes an already installed channel; adding components
alone does not establish that its compiler is current. Retain the compiler and
LLVM identities and the test's reported raw version/layout with the results.
Use rustup's command proxies on `PATH` and keep any explicit `RUSTC` setting
consistent with the selected toolchain. `RUSTUP_TOOLCHAIN` follows subprocesses
into temporary fixture repositories, where a directory-local override alone
does not apply. This keeps the policy helper's build-time and runtime compiler
identities aligned. The checks retain their compiler-identity refusals.

The `stable-repo-policy` job in the existing advisory Future Clippy workflow
runs the focused diagnostic tests, the isolated `frontdoor` target and Clippy on
the moving stable channel with matching `llvm-tools-preview`. It records the
actual compiler and LLVM tool versions. It follows that workflow's existing
main-push, manual-dispatch and
`clippy-future`/`full-ci` plus `merge-ready` PR routing. A failure stays visible;
the job does not become a required merge gate or run full workspace coverage.
The existing future-Clippy job continues to inspect deferred lints on Rust
1.95.0. This stable compatibility check does not change the repository pin or
MSRV.

To generate coverage and inspect artifacts locally, run the same supported
recipe as `.github/workflows/coverage.yml`: export the instrumentation
environment first, clean after that environment, run a plain instrumented
build and plain test run, then report. The explicit absolute
`RIPR_TEST_BINARY` binds the analyzer-consuming xtask tests to the actual
instrumented executable; a configured coverage target that does not name the
instrumented analyzer fails closed instead of substituting an ordinary
binary. `report` takes no build flags and, at this virtual workspace root,
covers all members even without `--workspace`.

```bash
set -euo pipefail
llvm_cov_env="$(cargo llvm-cov show-env --export-prefix)"
eval "$llvm_cov_env"
cargo llvm-cov clean --workspace
cargo build --workspace --all-features
export RIPR_TEST_BINARY="$CARGO_LLVM_COV_TARGET_DIR/debug/ripr"
test -f "$RIPR_TEST_BINARY"
"$RIPR_TEST_BINARY" --version
sha256sum "$RIPR_TEST_BINARY"
cargo test --workspace --all-features --tests
cargo llvm-cov report --lcov --output-path lcov.info
ls -lh lcov.info
```

The show-env output is captured in a standalone assignment before `eval`
so a producer failure propagates under `set -e` instead of being masked
by `eval`.

To view the LCOV report in a browser (if you have `genhtml` installed):

```bash
genhtml lcov.info -o coverage-report/
open coverage-report/index.html
```

## Future calibration

Threshold ratcheting should follow the strategy documented in [IMPLEMENTATION_CAMPAIGNS.md](../IMPLEMENTATION_CAMPAIGNS.md). Allow real data to accumulate on `main` before raising targets or making Codecov status blocking for branch protection.
