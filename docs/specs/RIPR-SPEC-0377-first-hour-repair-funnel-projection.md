# RIPR-SPEC-0377: First-hour repair funnel projection

Status: proposed

Owner: test-infra

Created: 2026-11-02

Linked issues:

- #6308 (this slice)
- #1693 (parent programme)
- #5286 (first-run walk receipts)
- #5299 (dx-scoreboard)
- #1702 (governed real repair attempts)

Support-tier impact:

- None. The projection reads committed receipt fragments and fixture corpora;
  it executes no repair, calls no provider, and makes no support promotion.
  [docs/status/SUPPORT_TIERS.md](../status/SUPPORT_TIERS.md)

Policy impact:

- None beyond writing advisory reports under `target/ripr/reports`.

## Problem

Install, first-run, RepairCard, and governed repair-attempt receipts exist,
but no versioned funnel joins them into one denominator for time and effort
to honestly verified improvement or a useful terminal limitation (#6308).

## Behavior

`cargo xtask repair-funnel-report` evaluates `fixtures/repair_funnel/controls/corpus.json`
through `first_hour_repair_funnel_inputs.v1` → `first_hour_repair_funnel.v1`:

- Stages appear only when an authoritative receipt fragment supplies them;
  missing stages are not inferred from prose or issue state.
- `time_to_verified_improvement` requires both project verification executed
  at declared strength and current static movement improved or closed.
- Cohorts (`deterministic_fixture`, `installed_deterministic_journey`,
  `blind_operator_journey`, `governed_real_repository_attempt`) stay separate.
- Planted controls reject verified improvement without verification, static
  improvement without verification, fixture-as-real claims, blind success
  with hidden assistance, and denominator drops.
- JSON and Markdown render from one normalized report struct.

`cargo xtask dx-scoreboard --boards repair --ingest` accepts
`first_hour_repair_funnel.v1` via `ingest:repair-funnel` metrics declared in
`benchmarks/dx_scoreboard/scoreboards.toml`. Zero selected cases remain
`not_measured`, never zero duration or success.

## Required Evidence

- `fixtures/repair_funnel/controls/corpus.json`
- `cargo test -p xtask --bin xtask repair_funnel`
- `cargo xtask repair-funnel-report`

## Non-Goals

- No new real-repository attempt execution, installed journey, or blind operator run.
- No support-tier or release verdict from funnel metrics alone.
- No automatic product optimization from this slice.

## Acceptance Examples

- Governed case with install + passing verification + improved static movement
  records `verified_improvement_or_closure` and measured install-to-verified ms.
- Verification pass with unchanged static movement exits `honest_static_limitation`
  without a verified-improvement stage.
- Incompatible clocks leave duration metrics `not_measured`.

## Test Mapping

- `xtask/src/reports/repair_funnel.rs::tests::*`
- `fixtures/repair_funnel/controls/corpus.json` (planted controls)

## Implementation Mapping

- `xtask/src/reports/repair_funnel.rs`
- `xtask/src/reports/dx_scoreboard.rs` (`repair_funnel_to_input`)
- `benchmarks/dx_scoreboard/scoreboards.toml` (`repair.*` metrics)
- `xtask/src/command.rs` / `xtask/src/dispatch.rs` (`repair-funnel-report`)

## Metrics

- `repair.time_to_verified_improvement_ms` (advisory, runner-dependent)
- `repair.time_to_first_honest_result_ms` (advisory, runner-dependent)
- `repair.selected_case_count` (population visibility)
