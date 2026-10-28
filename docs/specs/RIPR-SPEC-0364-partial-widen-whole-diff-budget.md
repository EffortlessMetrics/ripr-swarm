# RIPR-SPEC-0364: The partial-scope widen hint names the budget that selects the whole diff

Status: proposed

Owner: product / analysis

Created: 2026-10-28

Linked proposal:

- [RIPR-PROP-0019](../proposals/RIPR-PROP-0019-partial-diff-scope-contract.md)

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #10894

Linked PRs:

- #10895

Support-tier impact:

- No tier change. This changes the repair text of a `limited_partial_scope`
  run; it adds no class, oracle kind or relation reason.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.d/` per the fragment convention.
- No schema version bump: `PartialDiffScope.whole_diff_budgets` is
  internal; the rendered widen instruction is free text.

## Problem

A diff over the partial-selection budgets (RIPR-PROP-0019) is analyzed as a
bounded partition, and the widen instruction named only the minimum that
admits the next file. On a Django diff the hint read "raise
`RIPR_PARTIAL_DIFF_LINE_BUDGET` to at least 1015", but completing the run
needed 1041, so following the hint produced another partial run.

## Behavior

1. When the partial selector stops, it records the whole-diff budgets (the
   enabled routed changed files and their changed lines) if and only if both fit
   the ceilings the budgets are clamped to: the file total within the
   effective `RIPR_MAX_DIFF_INDEX_FILES` and the line total within the
   effective `RIPR_MAX_DIFF_CHANGED_RUST_LINES`.
2. The partial line budget is one aggregate over every routed language and
   is clamped to the Rust line limit, so the line total includes enabled
   preview-language files. Files of a disabled language are never selected
   and never stop the walk, so they are left out of both figures. A whole-diff line total above that
   ceiling cannot be granted by raising the budget, and no figure is named.
3. The widen instruction names `at least {next-file minimum}` as before and
   adds `({whole} to select the whole diff)` when the whole-diff value is larger
   than both the minimum and the current budget. A budget the next file does
   not need is still named, as `{env} to {whole} to select the whole diff`, when
   the whole diff needs it.
4. When no whole-diff figure is recorded, the instruction is the
   next-file-minimum hint alone.
5. Raising the budgets widens the changed-package set, and the Rust index
   guard (`RIPR_MAX_DIFF_INDEX_FILES`) counts indexed package files, not
   changed files. The Rust adapter computes the files the mode selects for
   every changed Rust file before dependent expansion; when that floor
   exceeds the guard, the whole-diff retry would be refused, so no figure is
   named.
6. The figure is worded as selecting the whole diff, not as completing the
   run: dependent packages the wider run adds can still meet the index
   guard, which refuses with `diff_scope_oversized` and names its own repair
   route. The same holds for the next-file minimum.

## Required Evidence

- Budgets equal to the recorded whole-diff figures select no partition (the
  run is complete) and budgets one line lower stay partial.
- A whole diff over the line ceiling records no figure and keeps the plain
  hint; at the ceiling it records the figure.
- A mixed-language diff whose Rust lines fit the Rust limit but whose total
  exceeds it records no figure, and a budget override to the total is
  clamped and stays partial.
- With a preview language disabled, the figures count enabled files only:
  budgets equal to them complete the run and one line less does not.
- A file-budget stop under a roomy line budget names both raises.
- Through the Rust adapter, three five-file crates with one changed file
  each under budgets (1, 3): a 7-file index guard names no figure, and a
  15-file guard names (3, 6).

## Non-Goals

- Predicting dependent-package expansion or narrowing for the wider run;
  that guard runs after selection and refuses with its own repair.
- Changing the next-file minimum, the selection order or the budgets'
  defaults.
- Per-language partial budgets.

## Acceptance Examples

1. Three Rust files of 35, 30 and 30 changed lines under budgets (7, 40):
   "raise RIPR_PARTIAL_DIFF_LINE_BUDGET to at least 65 (95 to select the
   whole diff), then re-run"; budgets (3, 95) select no partition.
2. The same files with the Rust line limit at 94: "raise
   RIPR_PARTIAL_DIFF_LINE_BUDGET to at least 65, then re-run".
3. Django 5c13b5a HEAD~20 at the default budgets, measured by hand with a
   release build of PR head 5176826c7 (the earlier "for the whole diff"
   wording): the hint named 1015 and 1041, and a re-run at 1041 was
   `complete_with_findings`. No repository test replays this measurement;
   examples 1 and 2 are the tested oracle.

## Test Mapping

- `crates/ripr/src/analysis/language/rust/mod.rs` tests:
  `partial_widen_minimums_admit_the_next_file_and_one_less_does_not`,
  `partial_widen_names_the_whole_diff_only_when_its_limits_can_grant_it`,
  `partial_widen_counts_every_language_against_the_line_clamp`,
  `partial_widen_omits_the_whole_diff_when_its_packages_exceed_the_index_guard`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/rust/mod.rs`:
  `PartialDiffBudgets.{file_limit,line_limit}`,
  `select_partial_diff_partition_with_identity` records
  `PartialDiffScope.whole_diff_budgets`, and
  `PartialDiffScope::budget_raise_instruction` renders it; the Rust
  adapter clears it when the whole diff's index floor exceeds the guard.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
