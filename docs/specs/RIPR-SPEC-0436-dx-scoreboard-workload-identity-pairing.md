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

## Required Evidence

- Matching workloads compare in both metric directions across analyzer
  revisions.
- Changing any identity field yields no trend.
- Legacy, malformed and duplicate identity never pair.
- Partial overlap pairs only matching workloads and lists every exclusion.

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

## Implementation Mapping

- `xtask/src/reports/dx_scoreboard/workloads.rs`: `pair`, `unique_identity`.
- `xtask/src/reports/dx_scoreboard.rs`: the comparison caller.
- `benchmarks/dx_scoreboard/README.md`: the workload identity note.

## Metrics

- `unit_test_pass_rate`: the mapped tests pass. Target: 100 percent.
