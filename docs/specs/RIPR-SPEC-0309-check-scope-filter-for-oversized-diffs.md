# RIPR-SPEC-0309: Check can analyze a named subset of an oversized diff

Status: proposed

Owner: product / analysis

Created: 2026-10-21

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #9277
- #9331
- #9340
- #13894

Linked PRs:

- None yet

Support-tier impact:

- No tier change. This narrows which changed files a diff-scoped run reads; it
  adds no class, oracle kind or relation reason.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No schema version bump. #13894 adds the advisory `requested_scope_excluded`
  limitation kind (a scope note that rides on a complete outcome, like
  `eol_only_churn`); the files an operator leaves out no longer reuse
  `language_scope_unsupported`.

## Problem

A diff that trips the changed-line cap was all-or-nothing: the operator could
not say "analyze only `src/lib.rs`", and checked-in generated or garbage `.rs`
files counted toward the cap that protects runner memory. Raising the cap
analyzed those files too.

Since #13641 the per-language changed-line caps no longer refuse a diff in
which some file fits: an over-limit file is left out of a bounded
`limited_partial_scope` run (`over_limit_files`). A scope still matters
there, because it decides which files are analyzed rather than disclosed as
uninspected, and the language-neutral parse guards still refuse outright.

## Behavior

`ripr check` accepts repeatable repository-relative `--path <pattern>` and
`--exclude <pattern>`. A pattern without `*` or `?` names a file or a directory
(`src/lib.rs`, `crates/api`); a pattern with them uses the shared `/`-separated
glob dialect. No `--path` admits every file the excludes do not remove;
`--exclude` wins over `--path`.

1. The filter is applied once at pipeline intake, before the diff-size guards
   and before any adapter, so the guards measure only the chosen files. It is
   applied as the diff parser registers each file section, so a section the
   scope rejects is not retained and does not count toward the changed-file
   bound (`RIPR_MAX_DIFF_INDEX_FILES`) either. The limitation reports the exact
   number of distinct files left out and names the first few. A malformed or
   conflicted hunk inside a removed file records no limitation, because it says
   nothing about the files still analyzed. When the
   chosen files alone exceed a guard, the guard fires with its existing text,
   which now also names `--path`/`--exclude`.
2. Every changed file the filter removes is named in one
   `requested_scope_excluded` scope note (stage `diff_load`, recovery `retry`;
   #13894). The operator asked for that scope, so the outcome describes the
   requested scope: a fully analyzed scoped run is `complete_*`
   (`analysis_complete: true`), the note is not an incomplete-analysis
   limitation, and `missed-txt`/`github` do not repeat it (a clean run stays
   zero bytes). A skipped file still never yields a finding or a
   `no_static_path`. The same holds for files a configured
   `[languages.rust] generated_file_patterns` or preview
   `excluded_file_patterns` entry leaves out. Built-in, non-configurable
   skips (generated-file names, generator headers, `cargo vendor` crates),
   unsupported languages, parse failures, budgets and refused files keep their
   incomplete kinds and stay fail-closed, also when a requested exclusion is
   present in the same run. A file matching both a built-in rule and a
   configured pattern is requested scope (note only). When a configured
   pattern leaves out every changed Rust file, there is nothing left to
   analyze and nothing the operator did not ask to skip: the run is
   `no_behavioral_candidates` (complete) with the note, and `missed-txt` is zero bytes.
3. A filter that leaves no changed file fails closed with
   `scope_filter_matched_nothing`, naming the flags and the files they removed.
   It is not a clean result. Malformed patterns (empty, absolute, containing
   `..`) and use with a repo-scoped format are usage errors (exit 2).
4. The filter is bound into the analysis input identity, so one diff under two
   scopes is two analysis subjects. An unrestricted filter leaves identities
   unchanged.
5. A scoped run cannot be combined with `--write-artifact`, `--gap-ledger` or
   `--output-dir`: those outputs are reusable as a whole-diff result. Per-file
   parser limitations (conflict markers, combined hunks) on a scoped-out file
   are dropped; diff-wide counters (renames, deletions, truncated sections)
   stay whole-diff and conservative.
6. Rust files with a generator header, `cargo vendor` crates and a repository
   root `target/` directory are skipped with a named limitation before the
   caps, as before; a nested `target` directory is ordinary source.
7. A scoped `ripr check` prints its `--path`/`--exclude` flags (shell-quoted)
   on the explain, context, and `check --json` drill-in commands. `ripr explain`
   and `ripr context` accept the same flags and apply the same intake filter,
   so a pasted drill-in re-analyzes the scoped files instead of the full
   oversized diff. `ripr repair` still names only the seam and `--root`: it
   reads the live tree and does not re-analyze the git diff.

## Required Evidence

- Oversized diff with one real edit: `--exclude` or `--path` yields the real
  finding, a scope note naming the skipped files, a complete outcome and
  exit 0.
- A fully tested change with `--exclude`, `--path` or a configured
  `generated_file_patterns` is `analysis_complete: true` and `missed-txt`
  prints zero bytes.
- Negative: no finding comes from a skipped file; a built-in generated-name
  skip, or an unanalyzable kept file beside a requested exclusion, keeps the
  outcome `partial_with_limitations`.
- review-comments reports `advisory` (not `incomplete`) and the gate does not
  fail closed for a complete outcome that carries only the scope note.
- A scope matching nothing, and one excluding everything, are errors.
- Scope keeping only the oversized file still refuses (`diff_scope_oversized`);
  beside a fitting file it is left out as over limit instead (#13641).
- A diff naming more files than the changed-file bound parses when the scope
  removes the excess, and the limitation reports the exact number removed.
- A scoped check's printed explain/context/`check --json` lines carry the
  same `--path`/`--exclude` flags and those commands accept them. Since
  #13641, dropping the flags from a printed explain degrades to a partial
  scope instead of tripping `diff_scope_oversized`, and the same explain
  excluding the finding's file instead cannot resolve it, so the printed
  scope stays load-bearing.

## Non-Goals

- `--path`/`--exclude` on `ripr repair` (repair reads the live tree and
  carries no diff scope).
- A scope for repo-scoped formats, the LSP or MCP.
- Treating a bare `vendor/` directory as vendored; only `cargo vendor` crates
  are, and `--exclude vendor` covers the rest.

## Acceptance Examples

1. `ripr check --base HEAD~1 --exclude src/big.rs --exclude blob.rs` on a
   branch with one `src/lib.rs` edit, a 200k-line `src/big.rs` and a 3 MB
   `blob.rs`: the `src/lib.rs` finding plus a scope note naming the two files.
2. `ripr check --path nowhere/**`: `scope_filter_matched_nothing`, exit 2.
3. `ripr check --path ../x`: usage error naming the pattern, exit 2.
4. After example 1, the printed `ripr explain ... --exclude src/big.rs
   --exclude blob.rs <id>` line explains the finding. Before #13641 the same
   line without the exclude flags failed with `diff_scope_oversized`; with
   only the changed-line cap tripped it now leaves `src/big.rs` out of a
   partial scope, while a diff over a language-neutral parse guard still
   refuses.

## Test Mapping

- `crates/ripr/src/analysis/diff_scope_filter.rs` tests (pattern semantics).
- `crates/ripr/src/analysis/pipeline.rs` tests:
  `scope_narrows_the_diff_before_the_size_guard_and_notes_what_it_left_out`,
  `configured_generated_pattern_is_a_scope_note_not_a_limitation`,
  `built_in_generated_name_skip_stays_a_scope_limitation`,
  `mixed_configured_and_built_in_skips_split_note_from_limitation`,
  `scope_that_keeps_only_the_oversized_files_still_hits_the_guard`,
  `scope_that_matches_no_changed_file_is_an_error_not_a_clean_result`,
  `excluding_every_changed_file_is_also_an_error`,
  `scope_binds_into_the_input_identity`,
  `oversized_file_sorting_first_is_left_out_of_a_partial_scope`.
- `crates/ripr/tests/cli_smoke.rs`:
  `check_scope_flags_turn_an_oversized_diff_into_a_complete_scoped_result_with_a_note`.
- `crates/ripr/tests/cli_smoke/requested_scope_complete.rs`:
  `exclude_flag_is_requested_scope_and_stays_complete`,
  `path_flag_is_requested_scope_and_stays_complete`,
  `configured_generated_pattern_is_requested_scope_and_stays_complete`,
  `built_in_generated_skip_stays_incomplete`,
  `requested_exclusion_does_not_hide_an_unanalyzed_kept_file`,
  `every_changed_file_matching_a_configured_pattern_is_complete_and_quiet`.
- `crates/ripr/src/analysis/pipeline.rs` tests:
  `diff_whose_every_rust_file_matches_a_configured_pattern_is_complete_with_note`,
  `file_matching_built_in_and_configured_rules_is_a_note_only`,
  `preview_excluded_file_pattern_is_a_scope_note_on_a_complete_outcome`.
- `crates/ripr/src/output/human/agent_tests.rs::agent_scope_note_line_is_single_and_not_counted_as_a_limitation`.
- `crates/ripr/src/output/review_comments/mod.rs::tests::review_comments_keeps_complete_outcome_with_requested_scope_note_advisory`.
- `crates/ripr/src/output/gate/tests.rs::given_real_review_comments_with_requested_scope_note_when_gate_evaluated_then_not_config_error`.
- `crates/ripr/src/app/navigation.rs::tests::finding_navigation_appends_path_and_exclude_from_the_run_scope`.
- `crates/ripr/src/cli/commands.rs::tests::explain_accepts_path_and_exclude_and_rejects_a_dotdot_pattern`.
- `crates/ripr/src/cli/commands/context.rs::tests::context_accepts_path_and_exclude_and_rejects_a_dotdot_pattern`.
- `crates/ripr/tests/scoped_drill_in.rs::scoped_check_prints_path_exclude_drill_ins_that_replay`.
- `crates/ripr/src/analysis/language/rust/mod.rs` tests:
  `committed_root_target_is_build_output_but_a_nested_target_module_is_not`.

## Implementation Mapping

- `crates/ripr/src/analysis/diff_scope_filter.rs`: `DiffScopeFilter`.
- `crates/ripr/src/analysis/pipeline.rs`: intake partition,
  `scope_matched_nothing_error`, `out_of_scope_limitation`,
  `configured_exclusion_note`.
- `crates/ripr/src/analysis_outcome.rs`: `AnalysisLimitationKind::RequestedScopeExcluded`.
- `crates/ripr/src/cli/commands/check.rs`: `--path`, `--exclude`.
- `crates/ripr/src/app/navigation.rs`: printed drill-in argv.
- `crates/ripr/src/cli/commands.rs`: `explain` parser and `bind_diff_scope_flags`.
- `crates/ripr/src/cli/commands/context.rs`: `context` parser.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
