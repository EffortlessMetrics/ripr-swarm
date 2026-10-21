# RIPR-SPEC-0292: mutation spot check admits calibration schema 0.2 and 0.3

Status: proposed

Owner: product / xtask

Created: 2026-10-21

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8996 (mutation-spot-check still gates on calibration schema 0.2)

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Schema-version admission is harness evidence hygiene:
  it decides which calibration documents the spot check will classify,
  not a new oracle kind, strength, class, relation reason or limitation
  kind.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No schema version bump. No new wire shape: the spot-check receipt stays
  `ripr-mutation-spot-check-v2`, and the calibration report it ingests is
  classified, never rewritten.

## Problem

`cargo xtask mutation-spot-check` gates `classify_records` on a single
hard-coded calibration schema version (`CALIBRATION_SCHEMA_VERSION = "0.2"`).
The calibrate clarity work (RIPR-SPEC-0286, PR #8988) moves the
mutation-calibration report schema to 0.3, adding `provenance` and
`how_to_read` without changing the joined-record shape the spot check
scores. Once #8988 lands, the spot check would refuse every report the
renderer produces. Because #8988 was still in flight when this was filed,
the fix must keep the current 0.2-emitting main working and keep working
after #8988 lands.

## Behavior

### Rule 1: admission by document version

The spot check admits a calibration document whose `schema_version` field
is one of the supported set — currently `0.2` and `0.3` — and classifies
it per the version recorded in the document. Both versions carry the same
joined-record shape (`matches`, `ambiguous_file_line_matches`,
`ambiguous_span_overlap_matches`, `unmatched_mutants`, and
`metrics.mutants_total`), so one classification path serves both; the
0.3 additions (`provenance`, `how_to_read`) are metadata the harness
does not score and need no handling.

### Rule 2: the supported set names the owner

xtask is a separate crate and cannot import the renderer's
`MUTATION_CALIBRATION_SCHEMA_VERSION` constant from
`crates/ripr/src/output/mutation_calibration.rs` (it is `pub(crate)` and
xtask must not depend on the product crate), so the supported set is a
duplicated constant with a comment pointing at the owning constant and
spec. A renderer schema bump is expected to update the set in the same
lane; the refusal message (Rule 3) names the owner so a forgotten update
is diagnosable from the error alone.

### Rule 3: unknown or older versions fail closed and actionable

A document at any other `schema_version` (older than 0.2, newer than the
supported set, or missing) is refused before any record is scored. The
refusal message names the version found in the document, the supported
versions, and the owning renderer constant (with its before/after
RIPR-SPEC-0286 values), and tells the operator to rebuild ripr and
re-run the spot check so the calibration JSON carries a supported schema.

## Required Evidence

- A calibration 0.2 document classifies exactly as before (existing
  0.2-synthesizing unit tests stay green unchanged in spirit).
- A calibration 0.3 document with the same joined-record shape
  classifies to identical dispositions.
- A document at 0.1 or at a version above the supported set is refused,
  and the refusal names the found version, the supported versions, and
  `MUTATION_CALIBRATION_SCHEMA_VERSION`.

## Non-Goals

- Importing or re-exporting the renderer's schema constant into xtask
  (a cross-crate dependency the workspace layout does not support); the
  duplicated-with-pointer constant is the deliberate design.
- Changing classification logic, dispositions, or scored verdict
  families; admission is a gate in front of the unchanged classifier.
- Changing report generation: the renderer's schema version, the
  0.3 additions, and any future bump remain owned by RIPR-SPEC-0286.
- Accepting versions older than 0.2: pre-0.2 reports lack the
  span-containment join, and the gate stays fail-closed for them.

## Acceptance Examples

1. `classify_records` receives a 0.2 calibration document with one
   `seam_id`-joined binary-operator match: one `canonical_precise`
   record is produced.
2. The same document with `schema_version` set to `0.3`: identical
   result.
3. The same document with `schema_version` set to `0.1` or `0.4`: an
   error naming the found version, the supported versions, and
   `MUTATION_CALIBRATION_SCHEMA_VERSION` in
   `crates/ripr/src/output/mutation_calibration.rs`.

## Test Mapping

- `xtask/src/reports/mutation_spot_check.rs`: examples 1 and 2
  (`only_canonical_joins_of_supported_families_are_scoreable`,
  `a_run_scoring_one_family_still_ingests` and
  `operator_token_is_a_diagnostic_that_cannot_veto_a_canonical_join`
  synthesize 0.2 documents;
  `calibration_schema_0_3_classifies_and_unknown_versions_are_refused`
  pins the 0.3 path and the 0.4 refusal), example 3
  (`calibration_without_span_joins_or_with_missing_records_is_refused`
  pins the 0.1 refusal message).

## Implementation Mapping

- `xtask/src/reports/mutation_spot_check.rs`:
  `SUPPORTED_CALIBRATION_SCHEMA_VERSIONS`,
  `classify_records` (schema admission gate and refusal message), unit
  tests `calibration_schema_0_3_classifies_and_unknown_versions_are_refused`
  and `calibration_without_span_joins_or_with_missing_records_is_refused`.

## Metrics

- `unit_test_pass_rate`: the schema-admission tests above are required
  Rust tests; a failure blocks merge.
