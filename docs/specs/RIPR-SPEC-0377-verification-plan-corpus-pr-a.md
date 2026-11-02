# RIPR-SPEC-0377: Verification plan corpus (PR A mechanics)

Status: proposed

Owner: test-infra

Created: 2026-11-02

Linked issues:

- #5173 (this slice: schema, mechanics validator, synthetic falsification
  matrix, and honest empty real denominators)
- #5162 (parent/controller: verification planning campaign)
- #5163–#5168 (upstream plan/route contracts; referenced, not duplicated)

Support-tier impact:

- None. Offline typed validation over committed fixture rows only.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- None. Reads fixtures and writes `target/ripr/reports` only.

## Problem

Change-verification planning cannot be evaluated honestly without a governed
denominator that binds selection, plan commitment, execution observations, and
independent judgment. #5173 PR A establishes the mechanics corpus and validator
before historical, mutation, or Stage A real rows land.

## Behavior

`cargo xtask verification-plan-corpus check|report` loads
`fixtures/verification-plan-corpus/corpus.json`, validates every mechanics row
through `assess_verification_plan_case`, checks committed expectations, and
projects `VerificationPlanCorpusReportV1` with synthetic rows separated from
real safety denominators. With zero real rows every real rate stays
`not_measured`.

## Required Evidence

- Committed fixture corpus covering all sixteen required mechanics scenario ids
  in `REQUIRED_VERIFICATION_PLAN_MECHANICS_SCENARIO_IDS`.
- `cargo test -p xtask verification_plan_corpus`.
- `cargo xtask verification-plan-corpus check` and `report`.

## Non-Goals

Historical commits, mutation-generated rows, Stage A cross-repository manifest,
planner/analyzer repair, CI policy admission, or release action (remaining #5173
slices).

## Acceptance Examples

- A synthetic row with plan digest matching `plan_identity` and non-zero
  executed subjects when targeted success is claimed is accepted.
- Wrong plan digest, zero-subject targeted success, or missing evidence digests
  reject the row.
- A fixture corpus with only synthetic accepted rows reports `real_rows: []` and
  `would_miss_rate.state: not_measured`.

## Test Mapping

- `xtask/src/verification_plan_corpus.rs::tests::*`
- `xtask/src/reports/verification_plan_corpus.rs::tests::*`
- `cargo xtask verification-plan-corpus check`

## Implementation Mapping

- `xtask/src/verification_plan_corpus.rs` — DTOs and counting law
- `xtask/src/reports/verification_plan_corpus.rs` — check/report projection
- `fixtures/verification-plan-corpus/corpus.json` — committed mechanics matrix

## Metrics

- Mechanics acceptance/rejection counts from the report DTO only; no aggregate
  accuracy or efficiency score.
