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

To generate coverage and inspect artifacts locally:

```bash
cargo build -p ripr
cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info
ls -lh lcov.info
```

Build `ripr` first. Tests that spawn the binary look for a pre-built
`target/debug/ripr`, and `cargo llvm-cov` builds into its own target directory,
so from a clean checkout those tests fail without it.

To view the LCOV report in a browser (if you have `genhtml` installed):

```bash
genhtml lcov.info -o coverage-report/
open coverage-report/index.html
```

## Future calibration

Threshold ratcheting should follow the strategy documented in [IMPLEMENTATION_CAMPAIGNS.md](../IMPLEMENTATION_CAMPAIGNS.md). Allow real data to accumulate on `main` before raising targets or making Codecov status blocking for branch protection.
