# RIPR-SPEC-0286: Empty-diff base candidate must be analyzable

Status: proposed

Owner: product / cli

Created: 2026-10-20

Linked issues:

- #9013 (empty-diff --base candidate can name a branch whose diff ripr
  itself refuses as oversized)
- #7644 (first-run empty-diff guidance, delivered)

Linked PRs:

- #9035

Support-tier impact:

- None. No command, output field, or vocabulary changes; only the
  eligibility and ranking of an already-printed advice candidate change.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

## Problem

When the resolved base equals HEAD, `ripr check` may advise passing
`--base <branch>` for another branch. The probe named the first branch
in ref order with a non-empty diff. On a repository with stale local
branches, following that advice could refuse with
`diff_scope_oversized`, so ripr recommended a command it would not run.

## Behavior

The shared probe `another_base_candidate` names a branch only when the
advised run would itself be analyzed:

- the branch shares history with HEAD (merge-base differs), is not an
  alias at HEAD, is not a descendant, and its root-scoped three-dot diff
  is non-empty and does not restore the merge-base tree;
- the diff's Rust changed lines (including rename destinations, brace
  and quoted numstat forms) stay within the changed-Rust-line guard
  (`RIPR_MAX_DIFF_CHANGED_RUST_LINES`);
- the diff's changed-file count stays within the parser guard
  (`RIPR_MAX_DIFF_INDEX_FILES`);
- submodule state is compared with `--ignore-submodules=none`, matching
  the analysis loader.

Among eligible branches, a Rust-bearing diff ranks before a non-Rust
one, then the smallest total changed-line count wins; ref order breaks
ties. When nothing is eligible, no candidate is named and the ordinary
commit/`--worktree` safe action owns the empty state.

## Required Evidence

- Red-to-green unit tests beside the probe for the oversized-line,
  oversized-file, rename-counting, and smallest-diff cases.
- Existing probe rejection tests unchanged (alias, descendant,
  tree-restoring range, outside-root changes).
- Live scratch-repo witness: the oversized branch is not advised and
  the output falls back to the commit/`--worktree` action.

## Test Mapping

- `another_base_candidate_rejects_a_diff_over_the_changed_line_guard`
- `another_base_candidate_rejects_a_diff_over_the_changed_file_guard`
- `another_base_candidate_counts_renamed_rust_lines`
- `another_base_candidate_prefers_the_smallest_analyzable_diff`
- `numstat_new_path_unwraps_rename_and_quote_forms`

## Acceptance Examples

- Clean checkout of the default branch with an ancient local branch
  whose diff exceeds the guards: no `--base` advice for that branch.
- Fresh repository where the user committed onto the base branch and a
  near ancestor branch exists: that branch is advised when its diff is
  analyzable.

## Non-Goals

- No change to guard limits, `diff_scope_oversized` rendering, exit
  codes, or static-exposure vocabulary.

## Implementation Mapping

- `crates/ripr/src/analysis/diff/load.rs` — `another_base_candidate`,
  `another_base_candidate_with_limits`, `numstat_new_path`.
- `crates/ripr/src/analysis/language/rust/mod.rs` — shared
  `diff_changed_rust_line_limit` guard.
- `crates/ripr/src/analysis/diff/parse.rs` — shared
  `diff_file_limit_from_env` guard.

## Metrics

- Gate: the five mapped tests pass; no advised candidate's root-scoped
  diff exceeds either guard.
