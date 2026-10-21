# RIPR-SPEC-0301: Direct-main release ledger contract and validator

Status: proposed

Owner: release / automation

Created: 2026-10-21

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #2768 (direct-main ledger for the 0.11 cut)

Linked PRs:

- None yet

Support-tier impact:

- None. This is repository release automation, not a product verdict surface.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- Adds the `release-ledger` entry to the command catalog.
- `RIPR-SPEC-0146` (`release-denominator`) stays as the historical C/T model.

## Problem

The 0.11 cut is the direct-main range `45b56c0957ad7e7360114edceca4b844c85f846e..cut`,
about two thousand six hundred commits. The historical denominator has an
eight-value vocabulary, no final-tree-effect axis, no open-PR rows and relies
on hand adjudication per commit. None of that fits the current #2768 contract.

## Behavior

`cargo xtask release-ledger --input <ledger.json>` reads one normalized ledger
and writes `target/ripr/reports/release-ledger.{json,md}` from that one DTO.
It exits nonzero when any reason is found. This piece validates a captured
ledger offline; Git capture, effect derivation and disposition rules are
separate pieces of #2768.

The ledger states `adjudication_basis: rule_derived`. It never claims
per-commit semantic review.

The ledger retains separately: the ordered first-parent sequence
(`range.first_parent`), the complete reachable set (`range.reachable`, in
first-parent order followed by the remaining commits sorted), merged PRs,
selected claims, open PRs at the cut, and the cut tree identity.

Closed dispositions (eleven): `included_accepted_product`,
`included_accepted_release_or_control`, `included_test_or_evidence`,
`included_structural_no_semantic_delta`, `included_superseded_by_later_fix`,
`included_historical_reachable_only`, `included_with_release_nonclaim`,
`not_in_cut_accepted_defer`, `not_in_cut_superseded`, `not_in_cut_unrelated`,
`blocking_unresolved`.

Final tree effect, a separate fact: `present_effective`,
`present_but_superseded`, `history_reachable_no_final_blob_effect`,
`not_reachable_from_candidate`. Each record carries an `effect_basis`
(alive paths, absent paths, last-toucher paths, blamed lines) so the effect can
be checked without re-reading Git.

The validator fails on:

1. a reachable commit with no record, a duplicate record, or a record naming a
   commit outside the reachable range;
2. a wrong first-parent position, a first parent that is not the preceding
   first-parent commit, or records outside canonical order;
3. a first-parent, reachable-set, record-set or tree digest that differs from
   the recomputed value, including a set with matching counts but different
   members;
4. a merged-PR row that contradicts Git (state not merged, merge commit outside
   the range, or a squash commit whose trailing PR is merged elsewhere);
5. selected-claim totals that differ from the rows or from the supplied #2766
   packet, or a packet digest mismatch;
6. a final tree effect that contradicts the record's `effect_basis`, or a
   disposition that contradicts the effect without an `override_ref`;
7. a free-form value in any closed field, or any `not_in_cut_*` disposition on a
   reachable commit;
8. any `blocking_unresolved` record or open PR in a `final` ledger, or a final
   ledger without an audit digest;
9. a Markdown or manifest projection whose digests differ from the ledger;
10. a cut that is not the observed tip of `main`.

Open-PR rows must not be inside the reachable range and may carry only
`not_in_cut_*` or `blocking_unresolved`; with a packet, the roster must match.

## Required Evidence

One negative control per rule above, each starting from a valid ledger, plus a
valid-ledger control that yields no reasons.

## Non-Goals

- Capturing Git facts or deriving tree effects and dispositions (later pieces).
- Selecting, qualifying, merging, tagging or publishing a candidate.
- Replacing `release-denominator` or editing SPEC-0146.

## Acceptance Examples

1. A ledger with one record removed and digests recomputed is invalid with
   `has no record`.
2. A reachable member swapped for another SHA, with the same counts, is invalid
   on the reachable digest.
3. A `final` ledger with one `blocking_unresolved` record is invalid.
4. A dry-run ledger with `blocking_unresolved` records is valid.

## Test Mapping

- `xtask/src/reports/release_ledger/tests.rs`: one test per rule, named for its
  failure (`missing_record_is_rejected`, `duplicate_record_is_rejected`,
  `record_outside_range_is_rejected`, `reordered_first_parent_is_rejected`,
  `count_consistent_but_wrong_set_is_rejected_by_digest`,
  `wrong_first_parent_digest_is_rejected`, `merged_pr_contradicting_git_is_rejected`,
  `selected_claim_totals_must_match_rows_and_packet`,
  `wrong_tree_effect_is_rejected`, `blocking_unresolved_fails_only_in_final`,
  `free_form_vocabulary_is_rejected`, `projection_digest_disagreement_is_rejected`,
  `cut_that_is_not_exact_main_is_rejected`,
  `open_pr_inside_range_or_with_included_disposition_is_rejected`,
  `valid_ledger_has_no_reasons`).

## Implementation Mapping

- `xtask/src/reports/release_ledger.rs`: command, arguments, JSON and Markdown.
- `xtask/src/reports/release_ledger/dto.rs`: DTO and closed vocabularies.
- `xtask/src/reports/release_ledger/validate.rs`: validator and digest recipes.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
