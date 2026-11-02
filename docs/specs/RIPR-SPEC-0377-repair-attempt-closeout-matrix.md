# RIPR-SPEC-0377: Repair attempt closeout matrix (R3, #4799)

Status: proposed

Owner: test-infra

Created: 2026-11-02

Linked issues:

- #4799 — durable-attempt crash/retry/compatibility matrix (this spec owns the
  normalized closeout DTO and row vocabulary)
- #2927 — parent repair-attempt transaction (consumer of the final matrix packet)
- #4797 / RIPR-SPEC-0195 — store identity (consumed)
- #4798 / RIPR-SPEC-0217 — attempt status selection (consumed)

Support-tier impact:

- None. The matrix is offline integration evidence; it does not promote a
  language, editor surface, gate, or public support claim.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and `.ripr/traceability.toml`.

## Problem

#4797 and #4798 landed typed store identity and exact attempt selection, and
`repair_attempt` unit/integration tests cover many failure modes in isolation.
Issue #4799 still needs one normalized closeout packet with a row for every
required scenario so #2927 can close or name one remaining defect without
hiding failures behind an aggregate pass.

## Behavior

`repair_attempt_closeout_matrix.v0.1` is the shared DTO behind human and JSON
closeout views. Each row records the matrix authority fields from #4799
(scenario identity, repository/store/attempt identity, before/after state,
injected fault or movement, observed status/currentness/movement/evidence,
expected next or recovery action, artifact/process disposition, platform/host
class, terminal `result`, limitations, non-claims, and optional owning issue on
failure).

The aggregate is `complete` only when every required row has terminal result
`pass` or explicit `incomparable` on each claimed platform. Rows marked
`missing_evidence` keep the aggregate `incomplete`; skipped or zero-subject
runs cannot aggregate to pass.

Slice `r3-slice-1-normal-use-linux` exercises the five normal/repeat-use
scenarios on Linux through production APIs; the remaining twenty rows stay
explicitly unexercised until later slices.

## Required Evidence

- `cargo test -p ripr --lib repair_attempt_closeout_matrix -- --nocapture`
  runs with nonzero subjects.
- The catalog pins exactly twenty-five scenario identities.
- Slice 1 leaves aggregate `incomplete` while twenty rows remain
  `missing_evidence`.
- JSON and Markdown renderings derive from one DTO and agree on aggregate
  state.

## Non-Goals

- Interruption/publication scenarios (issue #4799 sections 6–11) in slice 1
- Hosted Windows matrix receipts in slice 1
- Parent #2927 closeout packet completion
- Support-tier, release, or publication action

## Acceptance Examples

1. Row `normal.default_store_before_edit_after_status` reads `pass` on Linux
   with `finished_current` after terminal retention.
2. Row `normal.same_seam_distinct_attempt_ids` reads `pass` when finish
   refuses another attempt's packet.
3. The aggregate stays `incomplete` until all twenty-five rows read `pass`
   or declared `incomparable` on each claimed platform.

## Test Mapping

- `crates/ripr/src/app/repair_attempt_closeout_matrix.rs::tests::repair_attempt_closeout_matrix_catalog_has_twenty_five_scenarios`
- `crates/ripr/src/app/repair_attempt_closeout_matrix.rs::tests::repair_attempt_closeout_matrix_slice_one_exercises_normal_use_rows`
- `crates/ripr/src/app/repair_attempt_closeout_matrix.rs::tests::repair_attempt_closeout_matrix_json_and_markdown_agree_on_aggregate`
- `crates/ripr/src/app/repair_attempt_closeout_matrix.rs::tests::repair_attempt_closeout_matrix_unexercised_rows_stay_missing_evidence`
- `crates/ripr/src/app/repair_attempt_closeout_matrix.rs::tests::explicit_store_resolver_matches_manifest_store`

## Implementation Mapping

- `crates/ripr/src/app/repair_attempt_closeout_matrix.rs` — DTO, aggregate law,
  slice-1 runner, JSON/Markdown renderers
- `crates/ripr/src/app.rs` — module registration

## Metrics

- `unit_test_pass_rate` — slice-1 matrix tests pass on the candidate
