# RIPR-SPEC-0436: DX scoreboard workload identity pairing

Status: proposed

Owner: test-infra

Created: 2026-11-09

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #13177 (scoreboard workload identity)
- #14539 (Rust 1.100 compiler comparability boundary)

Linked PRs:

- #13182 (this spec and the `pair` rule)

Support-tier impact:

- None. This is repository automation (`xtask dx-scoreboard`), not product
  analysis. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.d/`, `docs/spec-index.d/` and
  `.ripr/traceability.d/`.
- No baseline schema change and no workflow change.

## Problem

A per-repository measured metric compared a current run against a baseline by
display id alone. A run on a different commit, base, URL or language of the same
repository, or on a malformed or duplicated identity, could therefore be
presented as a trend of the same workload.

## Behavior

### Rule 1: only an identical workload pairs

A per-repository measured sample pairs with the baseline sample of the same
repository only when both sides carry exactly one identity (url, sha, base and
language) and the two identities are equal. The analyzer revision may differ.

### Rule 2: every other case is excluded and named

Changed, legacy, malformed, missing or duplicate identity, and a repository
present on only one side, never pair by display id. Each is listed as an
exclusion with its reason, including new and missing repositories.

### Rule 3: exclusions never hide gaps

An excluded workload contributes no trend. Absolute ceilings, other metric
sources and incomplete-measurement reporting stay in force, and a missing or
changed identity cannot hide an incomplete measurement.

### Rule 4: measured speed requires the same recorded build compiler

The report header's additive `rustc` field records the release, full source
commit, compiler host and LLVM version of Cargo's selected compiler. The
scoreboard's default build joins the exact `ripr` binary artifact to its
package's build-script environment and successful Cargo completion. Cargo
emits cached build-script output for a fresh artifact, so a reused binary
retains its producer's compiler record. The build script queries Cargo's
`RUSTC`; the scoreboard never infers an external binary's compiler from PATH.

Only `source = "measured"` metrics on the speed board require two complete,
equal compiler records for a numeric trend. A changed, absent, malformed or
ambiguous compiler record excludes numeric comparisons, including numeric
regressions accompanying a lost completion. Completion losses, failed
instruments and absolute ceilings remain failures. Other metric sources keep
their existing comparisons; ingest admission refuses to overwrite a measured
metric, including measured speed.

Reports remain `ripr-dx-scoreboard-v1`. Old reports and explicit `--ripr-bin`
runs have unknown compiler provenance and cannot establish a numeric speed
trend. Unknown does not equal unknown. Existing historical baselines are not
backfilled. An absent or ambiguous artifact/completion join refuses the owned
build; unknown compiler metadata alone remains visible as an uncompared run.

The producer contract is Cargo's documented
[JSON messages](https://doc.rust-lang.org/cargo/reference/external-tools.html)
and build-script
[`RUSTC` environment](https://doc.rust-lang.org/cargo/reference/environment-variables.html),
not Cargo's intermediate directory layout or a new product CLI protocol.

## Required Evidence

- Matching workloads compare in both metric directions across analyzer
  revisions.
- Changing any identity field yields no trend.
- Legacy, malformed and duplicate identity never pair.
- Partial overlap pairs only matching workloads and lists every exclusion.
- Cold and fresh Cargo artifacts join only their own package's complete
  compiler record; missing or ambiguous metadata remains unknown.
- Matching compilers preserve speed regression failures; changed and legacy
  compilers suppress numeric trends without hiding completion loss or ceilings.
- Ingested speed rows cannot overwrite owned measured rows and do not borrow
  the local binary's compiler identity.

## Non-Goals

- Sampling-method validity or any claim about runtime mutation outcomes.
- Refreshing a baseline or editing a workflow.

## Acceptance Examples

1. Same repository, equal identity, new analyzer revision: pairs and may
   regress or improve.
2. Same repository with a different sha: excluded, no trend.
3. Repositories a and b in the baseline, a and c now: only a pairs; b and c are
   listed as missing and new.

## Test Mapping

- `xtask/src/reports/dx_scoreboard/workload_tests.rs`: the examples above.
- `xtask/src/reports/dx_scoreboard/compiler_tests.rs`: compiler comparison,
  legacy report, completion loss, ceiling and ingested-source controls.
- `xtask/tests/dx_scoreboard_compiler.rs`: the production Cargo evidence join
  and complete compiler identity rule, with no automation dependency graph.
- `xtask/tests/dx_scoreboard_terminal_evidence.rs`: explicit binaries retain
  an unknown compiler in the native scoreboard report.

## Implementation Mapping

- `xtask/src/reports/dx_scoreboard/workloads.rs`: `pair`, `unique_identity`.
- `xtask/src/reports/dx_scoreboard.rs`: the comparison caller.
- `xtask/src/reports/dx_scoreboard/compiler.rs`: the Cargo evidence join and
  shared compiler comparison rule.
- `xtask/src/reports/dx_scoreboard/measure.rs`: owned Cargo capture and binary
  selection.
- `crates/ripr/build.rs` and `crates/ripr/src/build_compiler_record.rs`: the
  Cargo-selected compiler record producer and shared admission rule.
- `benchmarks/dx_scoreboard/README.md`: the workload identity note.

## Metrics

- `unit_test_pass_rate`: the mapped tests pass. Target: 100 percent.
