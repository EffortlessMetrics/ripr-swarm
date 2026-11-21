# RIPR-SPEC-0451: DX scoreboard generates recall-of-missed and no-seam rows

Status: proposed

Owner: test-infra

Created: 2026-11-21

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #12900 (generate the mutation recall-of-missed and no-seam trust rows)

Linked PRs:

- #15853 (this spec and the generated rows)

Support-tier impact:

- None. This is repository automation (`cargo xtask dx-scoreboard`), not
  product analysis. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.d/`.
- No baseline schema change and no workflow change.

## Problem

Three trust rows were hand-derived from the mutation spot-check receipt:
strict recall of missed mutants, flagged-or-unknown recall of missed
mutants, and the no-seam share. A hand-derived share can drift from the
counts it claims to summarise, and nothing failed when it did. The receipt
already carries the counts, so the scoreboard can compute the rows itself.

## Behavior

### Rule 1: rows come from counts, never from stored shares

`mutation_spot_check_to_input` computes each of the three rows from the
receipt's own counts (`recall_of_missed.ungripped_missed` over
`missed_total`, `recall_of_missed.flagged_or_unknown_missed` over
`missed_total`, and `seam_coverage.no_containing_seam` over
`mutants_total`). A stored share that disagrees with the counts by more
than the receipt's three-decimal rounding fails ingest instead of
trending.

### Rule 2: an absent section omits the row; an impossible one fails

A receipt written before a section existed omits that row. A present
section with a missing, non-integer, or impossible count is an error. A
positive numerator over a zero denominator is impossible and fails; both
counts zero claims no figure and omits the row. A present section that is
not a JSON object fails with a message naming the section, not a generic
missing-key error.

### Rule 3: the three generated rows are optional to a configuration

A `--config` written before these rows existed never declared them, and
ingest rejects an undeclared metric. The projection therefore drops a
generated row the selected configuration does not declare, so an older
configuration still loads the same receipt. Every other row a receipt
names stays in the projection, so an undeclared pre-#12900 metric still
fails ingest rather than disappearing from the scoreboard.

### Rule 4: strict recall is informational

`trust.mutation_recall_of_missed` carries `target = 0` and is titled
informational. The 80% replacement threshold belongs to
`trust.mutation_recall_flagged_or_unknown`, which is the row the
replacement criteria reserve it for.

## Required Evidence

- The checked-in receipt generates 34/322 strict recall, 189/322
  flagged-or-unknown and 843/1745 no-seam.
- A configuration without the three declarations still loads that receipt
  and keeps the pre-#12900 rows.
- A stored share that disagrees with the counts fails ingest.
- A positive numerator over a zero denominator fails ingest.
- A non-object section fails with a message naming the section.

## Non-Goals

- Changing the spot-check receipt schema or the producer.
- Refreshing a baseline or editing a workflow.
- Any claim about runtime mutation outcomes.

## Acceptance Examples

1. The checked-in receipt yields the three rows at the counts above.
2. The same receipt against a configuration that declares none of the
   three loads, and the three rows are absent.
3. `ungripped_missed = 1`, `missed_total = 0` fails with
   `counts 1 of only 0`.
4. `ungripped_share_of_missed = 0.5` against `1/4` fails with
   `disagrees with`.
5. `recall_of_missed = "n/a"` fails with `must be a JSON object`.

## Test Mapping

- `xtask/src/reports/dx_scoreboard/tests.rs::mutation_spot_check_checked_in_receipt_generates_issue_12900_values`
  - example 1.
- `xtask/src/reports/dx_scoreboard/tests.rs::a_config_without_the_generated_rows_still_loads_the_checked_in_receipt`
  - example 2.
- `xtask/src/reports/dx_scoreboard/tests.rs::mutation_spot_check_receipt_generates_recall_and_no_seam_rows`
  - examples 3 and 4.
- `xtask/src/reports/dx_scoreboard/tests.rs::a_non_object_spot_check_section_fails_with_its_name`
  - example 5.

## Implementation Mapping

- `xtask/src/reports/dx_scoreboard.rs::mutation_spot_check_to_input` - the
  three generated rows.
- `xtask/src/reports/dx_scoreboard.rs::spot_check_ratio` - count and share
  validation.
- `xtask/src/reports/dx_scoreboard.rs::project_declared_optional_rows` -
  the configuration-dependent projection.
- `benchmarks/dx_scoreboard/scoreboards.toml` - the three declarations and
  the informational target.

## Metrics

- `trust.mutation_recall_of_missed` (informational, target 0).
- `trust.mutation_recall_flagged_or_unknown` (target 0.8).
- `trust.mutation_no_seam_share` (target 0.5).
