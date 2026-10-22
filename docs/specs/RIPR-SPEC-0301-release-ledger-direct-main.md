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
It exits nonzero when any reason is found. Validation is offline; capture and
the disposition rule are described below.

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
8. any `blocking_unresolved` record, non-terminal open PR or `pending`/`failed_release_hold` claim in a `final` ledger, or a final
   ledger without an audit digest, or a final ledger validated without `--packet`;
9. a Markdown or manifest projection whose digests differ from the ledger;
10. a cut that is not the observed tip of `main`;
11. a base other than the pinned 0.11 base (`--expected-base`, default
    `45b56c09…`), a reachable commit with no parent path from the cut, or a cut
    tree that differs from the cut record's tree;
12. a merged-PR row that no first-parent record names, an unsupported
    `rule_version`, a blank `override_ref`, a no-blob-effect row that claims an
    owned line, or a Markdown projection with a second, conflicting digest line.

Open-PR rows must not be inside the reachable range and carry the #2766 PR
dispositions from `policy/release-selection.json`; with `--packet`, the claim
rows, totals, packet digest (sha256 of the raw file bytes) and open-PR roster
must match it, and a final ledger needs the roster captured at the cut commit.
`history_reachable_no_final_blob_effect` requires no alive touched path (an
empty diff has none).

## Git capture and tree effect

`cargo xtask release-ledger capture --base <sha> [--cut <rev>] [--main <rev>]
[--repo <dir>] [--repository <owner/name>] --output <facts.json>` reads only
local Git and writes a `release_ledger_git_facts` document: the exact
`base..cut` first-parent sequence, the reachable set, one fact per reachable
commit (parents, tree, subject, changed paths, `final_tree_effect`,
`effect_basis`), and the merged-PR rows taken from trailing `(#N)` subject refs
of first-parent commits. Other `#N` mentions are kept as `issue_refs` because
Git cannot tell an issue from a PR.

Final tree effect is derived exactly and cheaply first:

1. Paths are the changes against each commit's first parent, without rename
   detection (a rename is a delete plus an add).
2. The last first-parent toucher of a path alive at the cut owns that path's
   final blob (`last_toucher_paths`).
3. `git blame` at the cut runs, in parallel, only for alive paths with more
   than one toucher where some toucher is not yet known to own something, and
   for every alive path a detected rename moved into (its older lines were
   written under another name). A
   commit owning a surviving line gets `blamed_lines`.
4. A commit that owns a path or line is `present_effective`; otherwise one with
   an alive touched path is `present_but_superseded`; otherwise
   `history_reachable_no_final_blob_effect`.

Tree effect follows the renames git detects (`-M`, default similarity). A blame failure, a timeout or a non-UTF-8 path fails the capture; an unknown
effect is never defaulted.

Each fact also carries `inline_test_paths`: the product-class `.rs` paths whose
every change in that commit sits inside a `#[cfg(test)]` item, on each side that
has lines. It is read from `git diff -U0` hunks against the first parent and the
item spans of the file at the parent and at the commit. Spans come from a lexer
that ignores braces in comments, strings, raw strings and char literals; an item
it cannot close, a whole-file add or delete, a hunk-less change and a quoted path
give no span and stay product, as does an attribute on a field, variant or
parameter (no item of its own ends before an enclosing closer). A span whose first or last line also holds other code
gives no span either, because hunks are whole lines. A hunk that touches a span's
attribute or closing line is a product change unless it is exactly the whole span, so a hunk that also adds a blank separator line
around a new test module stays product (a conservative miss).
 Product-class Rust under
`editors/` and `schemas/` is scanned as well as `crates/ripr/src/`.

## Disposition rule v1 and overrides

`cargo xtask release-ledger derive --facts <facts.json> --packet
<release-selection.json> [--overrides <overrides.json>] [--audit <file>]
[--stage dry_run|final] --output <ledger.json>` turns captured facts and the
#2766 packet into a ledger, then validates it. `rule_version` is `v1`, every
row's `acceptance_owner` is `rule:ledger-v1` unless an override names another
owner, and `source_sync_authority_effect` is `not_established` because the
source-parent path inventory is not an input of v1.

Path classes: `product` is `crates/ripr/src/`, `editors/` and `schemas/` (the
published wire contract), except that any `.md` is `docs`, `package-lock.json`
is `control`, and `test/`, `testing/` and `test-fixtures/` paths are `evidence`; `evidence` is any test path (`tests/`, `tests.rs`,
`*_tests.rs`, `fixtures/`), `benchmarks/`, `metrics/`, `docs/specs/` and
`.ripr/`; `docs` is `docs/`, `changelog.d/`, any `.md` and the root `LICENSE`,
`LICENSE-MIT` and `LICENSE-APACHE`; `automation` is `xtask`; `control` is
`.github`, `.allow`, `.agents`, `.claude`, `.config`, `.cargo`, `.ci`,
`policy`, `scripts`, `tools`, `packaging`, `npm`, the rest of `crates/` and other root
files; anything else is unclassified.

A product path listed in the commit's `inline_test_paths` counts as `evidence`, so
a commit whose only product edits are inline tests is `included_test_or_evidence`,
while one that also edits product code in the same file stays product.

Order, first match wins:

0. A commit with an empty diff (first-parent or not), a first-parent commit with
   two parents, or a first-parent commit with any unclassified path (even beside a
   classified one) is `blocking_unresolved`.
1. A commit reached only through a merge (not first-parent) is
   `included_historical_reachable_only`; its tree effect stays a separate field.
2. `present_but_superseded` is `included_superseded_by_later_fix`.
3. `history_reachable_no_final_blob_effect` is `included_historical_reachable_only`.
4. A non-product commit whose subject names the owner issue of a packet claim
   resolved `accepted_defer_with_non_claim` is `included_with_release_nonclaim`,
   with `claim_ref`. A product-class commit naming that issue landed behavior
   and takes rule 5, so a fix is never reported as a non-claim.
5. Otherwise the strongest class present: product is
   `included_accepted_product`, control is
   `included_accepted_release_or_control` (automation, `xtask`, keeps that
   disposition under its own `automation` theme; a commit that also touches control
   paths takes the control theme), evidence is `included_test_or_evidence`,
   docs is `included_structural_no_semantic_delta`.

The design brief reads "only tests..." and "only docs..." for the lower classes;
v1 takes the strongest class present, so a tests-plus-docs commit is evidence
rather than unresolved. A first-parent commit with no trailing `(#N)` is
classified normally with a limitation, and carries no `protected-squash` proof
ref.

`blocking_unresolved` rows are resolved by `policy/release-ledger-overrides.json`
(`kind: release_ledger_overrides`): `{commit_sha, disposition, owner, note}`,
reviewed in a PR. An override must name a commit in the range, a closed
non-blocking disposition, an owner and a note, may appear once, and may name only a
commit the rule left `blocking_unresolved`. The row keeps
the rule's disposition in its limitations and carries `override_ref`. A refuted
rule changes `rule_version` and regenerates; it never edits rows by hand.

## Producing the final ledger

At the frozen cut (`FINAL_SWARM_CUT`, #2768; the cut checklist owns the freeze),
with the final `policy/release-selection.json` and the completed sample audit
committed or otherwise retained:

```text
cargo xtask release-ledger finalize \
  --cut <full 40-hex FINAL_SWARM_CUT> \
  --packet policy/release-selection.json \
  --audit <audit file> \
  --overrides policy/release-ledger-overrides.json
```

The command runs four steps and stops at the first failure: `capture` of
`--base..--cut` with `--main` equal to the cut (the open-PR roster is read at the
cut, as a final ledger requires), `derive --stage final` with the audit digest
recorded, a Markdown projection rendered from the same ledger, and `--final`
validation against the packet, the Markdown and `--manifest` when given.
`--base` defaults to the pinned 0.11 base. `--cut` must be a full commit id;
a moving ref such as `origin/main` is rejected. Outputs land in `--outdir`
(default `target/ripr/release-ledger/final`): `facts.json`, `ledger.json` and
`ledger.md`; the validation report is written to
`target/ripr/reports/release-ledger.{json,md}`. Success means a valid final
ledger: every row has a closed non-blocking disposition and the audit file still
matches its recorded digest. A `blocking_unresolved` row fails the run; resolve it
with a reviewed override and rerun, never by editing `ledger.json`. The command
does not freeze, select, qualify, tag or publish.

## Required Evidence

One negative control per rule above, each starting from a valid ledger, plus a
valid-ledger control that yields no reasons.

## Non-Goals

- The dry run and its independent sample audit (a later piece).
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

- `xtask/src/reports/release_ledger/inline_tests_tests.rs`:
  `cfg_test_modules_and_items_are_spans`,
  `braces_in_strings_chars_comments_and_raw_strings_do_not_end_a_span`,
  `attributes_in_strings_and_other_cfgs_are_not_spans`,
  `an_unclosed_item_is_not_a_span`, `zero_context_diffs_parse_hunks_and_whole_files`,
  `quoted_paths_are_never_inline_edits`,
  `hunks_must_sit_inside_a_span_on_every_side_that_has_lines`,
  `only_product_rust_files_are_candidates`,
  `a_span_sharing_a_line_with_other_code_is_not_a_span`,
  `touching_a_span_boundary_line_is_a_product_change`,
  `an_escaped_quote_char_literal_is_blanked_whole`,
  `cfg_test_on_a_field_or_parameter_is_not_an_item_span`.
- `xtask/src/reports/release_ledger/capture_tests.rs`:
  `inline_test_edits_are_detected_per_hunk_from_real_git`,
  `editor_rust_is_scanned_and_same_line_product_edits_stay_product`,
  `changing_which_code_is_cfg_test_gated_is_not_an_inline_test_edit`,
  `capture_derives_exact_range_and_tree_effects` (squash, superseded line,
  deleted path, non-squash merge with a second-parent commit) and
  `split_refs_takes_only_the_trailing_pull_request`,
  `lines_written_under_a_renamed_path_are_attributed_by_blame`.
- `xtask/src/reports/release_ledger/rule_tests.rs`:
  `path_classes_follow_the_published_table`, `rule_steps_apply_in_order`,
  `unjudgeable_commits_block_instead_of_defaulting`,
  `deferred_claim_owner_issue_in_subject_gives_a_nonclaim_row`,
  `derived_ledger_validates_and_overrides_resolve_blocking_rows`,
  `bad_overrides_are_rejected`, `short_commit_ids_do_not_panic_the_override_reference`,
  `repeated_derive_flags_are_rejected_before_any_file_is_read`,
  `failed_derivation_leaves_an_existing_output_untouched`,
  `xtask_only_commits_share_the_control_disposition_under_their_own_theme`,
  `inline_test_only_product_paths_count_as_evidence`.
- `xtask/src/reports/release_ledger/tests.rs`: `finalize_defaults_to_the_pinned_base_and_output_directory`,
  `finalize_requires_the_frozen_cut_as_a_full_sha`,
  `finalize_requires_cut_packet_and_audit`,
  `finalize_rejects_unknown_repeated_and_valueless_flags`; and one test per rule, named for its
  failure (`missing_record_is_rejected`, `duplicate_record_is_rejected`,
  `record_outside_range_is_rejected`, `reordered_first_parent_is_rejected`,
  `count_consistent_but_wrong_set_is_rejected_by_digest`,
  `wrong_first_parent_digest_is_rejected`, `merged_pr_contradicting_git_is_rejected`,
  `selected_claim_totals_must_match_rows_and_packet`,
  `wrong_tree_effect_is_rejected`, `blocking_unresolved_fails_only_in_final`,
  `free_form_vocabulary_is_rejected`, `projection_digest_disagreement_is_rejected`,
  `cut_that_is_not_exact_main_is_rejected`,
  `open_pr_inside_range_or_with_included_disposition_is_rejected`,
  `packet_roster_and_final_capture_commit_are_compared`,
  `final_ledger_rejects_pending_claims_and_open_non_terminal_prs`,
  `commit_with_no_parent_path_from_the_cut_is_rejected`,
  `cut_tree_contradicting_the_cut_record_is_rejected`,
  `later_base_is_rejected_when_the_release_base_is_pinned`,
  `unlinked_merged_pr_row_is_rejected`,
  `history_row_claiming_surviving_lines_is_rejected`,
  `blank_override_ref_is_rejected`, `unknown_rule_version_is_rejected`,
  `markdown_with_a_conflicting_second_digest_line_is_rejected`,
  `markdown_projection_shows_every_records_theme`,
  `input_over_the_byte_cap_is_rejected_not_truncated`,
  `audit_file_digest_must_match_the_recorded_sha`,
  `editing_the_audit_file_after_derivation_fails_validation`,
  `product_commit_naming_a_deferred_claim_owner_issue_is_not_a_nonclaim`,
  `merge_only_commit_must_be_historical_and_owned_lines_need_no_alive_path`,
  `valid_ledger_has_no_reasons`).

## Implementation Mapping

- `xtask/src/reports/release_ledger.rs`: command, arguments, JSON and Markdown.
- `xtask/src/reports/release_ledger/dto.rs`: DTO and closed vocabularies.
- `xtask/src/reports/release_ledger/validate.rs`: validator and digest recipes.
- `xtask/src/reports/release_ledger/capture.rs`: Git capture and tree effect.
- `xtask/src/reports/release_ledger/rule.rs`: path classes, rule v1, overrides.
- `policy/release-ledger-overrides.json`: reviewed overrides for rows the rule cannot judge.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
