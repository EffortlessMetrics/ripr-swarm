# RIPR-SPEC-0372: Scoped gate acknowledgements

Status: proposed

Owner: product / output

Created: 2026-11-02

Linked issues:

- #1966
- #1965
- #1935

## Problem

Under acknowledgeable gate mode, a single PR acknowledgement label currently
acknowledges every blocking candidate (#1935). Reviewers need structured records
that bind to explicit canonical gap identity without silently waiving unrelated
gaps (#1966).

## Behavior

Gate evaluation accepts an optional `--acknowledgements` TOML ledger
(`schema_version = 1`) with `[[acknowledgements]]` rows. Each row carries
`acknowledgement_id`, `canonical_gap_id`, optional `seam_id`, `reason`, `actor`,
`created_at`, optional `expires_at` / `review_after`, and `max_matches`
(default 1).

Matching runs after candidate identity is known:

- a valid structured match on a blocking candidate yields `acknowledged` with
  policy metadata (`acknowledgement_id`, actor, reason, expiry/review fields,
  `acknowledgement_global_scope = false`);
- a blocking candidate with no structured match stays `blocking` unless a
  configured legacy label is present, in which case the decision is
  `acknowledged` with `acknowledgement_global_scope = true` and
  `acknowledgement_match_kind = legacy_acknowledgement_label`;
- expired or over-`max_matches` structured matches on an otherwise matching row
  yield `blocking` with `acknowledgement_limitation` on the decision policy;
- a missing, malformed, or root-mismatched supplied ledger is `config_error` and
  must not silently fall back to label-only global waiver.

## Required Evidence

- `cargo test -p ripr output::gate -- --test-threads=1`
- Updated boundary-gap gate-decision fixtures when legacy acknowledgement JSON
  shape changes

## Non-Goals

- Replacing PR labels as the visible trigger for legacy compatibility
- Durable repository acknowledgement ledgers outside explicit gate input
- Suppression or baseline semantics (remain separate policy states)

## Acceptance Examples

Given one structured acknowledgement for `gap:dedf923a13a00573` and one
blocking candidate with that canonical id, the gate decision is
`acknowledged` with the structured record id and without global scope.

Given the same ledger and a blocking candidate with a different canonical id,
the decision is `blocking` and summary blocking count stays non-zero.

Given only `ripr-waive` and no structured ledger, every blocking candidate is
`acknowledged` with `acknowledgement_global_scope = true`.

## Test Mapping

- `crates/ripr/src/output/gate/tests.rs`:
  `gate_structured_acknowledgement_scopes_to_matching_canonical_gap`
- `crates/ripr/src/output/gate/tests.rs`:
  `gate_structured_acknowledgement_does_not_ack_unrelated_canonical_gap`
- `crates/ripr/src/output/gate/tests.rs`:
  `gate_malformed_structured_acknowledgement_is_config_error`
- `crates/ripr/src/output/gate/structured_acknowledgements.rs` unit tests

## Implementation Mapping

- `crates/ripr/src/output/gate/structured_acknowledgements.rs`
- `crates/ripr/src/output/gate.rs`
- `crates/ripr/src/cli/commands/gate.rs` (`--acknowledgements`)

## Metrics

- Gate summary `blocking` vs `acknowledged` counts remain the authoritative
  per-run tallies for generated CI and CLI exit mapping.
