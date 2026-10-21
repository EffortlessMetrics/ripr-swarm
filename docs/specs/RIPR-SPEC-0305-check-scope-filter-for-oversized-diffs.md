# RIPR-SPEC-0305: Check can analyze a named subset of an oversized diff

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

Linked PRs:

- None yet

Support-tier impact:

- No tier change. This narrows which changed files a diff-scoped run reads; it
  adds no class, oracle kind or relation reason.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No schema version bump. The skipped files reuse the existing
  `language_scope_unsupported` limitation kind.

## Problem

A diff that trips the changed-line cap was all-or-nothing: the operator could
not say "analyze only `src/lib.rs`", and checked-in generated or garbage `.rs`
files counted toward the cap that protects runner memory. Raising the cap
analyzed those files too.

## Behavior

`ripr check` accepts repeatable repository-relative `--path <pattern>` and
`--exclude <pattern>`. A pattern without `*` or `?` names a file or a directory
(`src/lib.rs`, `crates/api`); a pattern with them uses the shared `/`-separated
glob dialect. No `--path` admits every file the excludes do not remove;
`--exclude` wins over `--path`.

1. The filter is applied once at pipeline intake, before the diff-size guards
   and before any adapter, so the guards measure only the chosen files. When the
   chosen files alone exceed a guard, the guard fires with its existing text,
   which now also names `--path`/`--exclude`.
2. Every changed file the filter removes is named in one
   `language_scope_unsupported` limitation (stage `diff_load`, recovery
   `retry`). The outcome is `partial_with_limitations`; a skipped file never
   yields a finding, a `no_static_path`, or a clean or complete outcome.
3. A filter that leaves no changed file fails closed with
   `scope_filter_matched_nothing`, naming the flags and the files they removed.
   It is not a clean result. Malformed patterns (empty, absolute, containing
   `..`) and use with a repo-scoped format are usage errors (exit 2).
4. The filter is bound into the analysis input identity, so one diff under two
   scopes is two analysis subjects. An unrestricted filter leaves identities
   unchanged.
5. Rust files with a generator header, `cargo vendor` crates and a repository
   root `target/` directory are skipped with a named limitation before the
   caps, as before; a nested `target` directory is ordinary source.

## Required Evidence

- Oversized diff with one real edit: `--exclude` or `--path` yields the real
  finding, a limitation naming the skipped files, and exit 0.
- Negative: no finding comes from a skipped file and the outcome is not
  complete.
- A scope matching nothing, and one excluding everything, are errors.
- Scope keeping the oversized file still trips the cap.

## Non-Goals

- Narrowing the parse-time changed-file-count bound
  (`RIPR_MAX_DIFF_INDEX_FILES`), which runs before the filter.
- A scope for repo-scoped formats, the LSP or MCP.
- Treating a bare `vendor/` directory as vendored; only `cargo vendor` crates
  are, and `--exclude vendor` covers the rest.

## Acceptance Examples

1. `ripr check --base HEAD~1 --exclude src/big.rs --exclude blob.rs` on a
   branch with one `src/lib.rs` edit, a 200k-line `src/big.rs` and a 3 MB
   `blob.rs`: the `src/lib.rs` finding plus a limitation naming the two files.
2. `ripr check --path nowhere/**`: `scope_filter_matched_nothing`, exit 2.
3. `ripr check --path ../x`: usage error naming the pattern, exit 2.

## Test Mapping

- `crates/ripr/src/analysis/diff_scope_filter.rs` tests (pattern semantics).
- `crates/ripr/src/analysis/pipeline.rs` tests:
  `scope_narrows_the_diff_before_the_size_guard_and_discloses_what_it_left_out`,
  `scope_that_keeps_only_the_oversized_files_still_hits_the_guard`,
  `scope_that_matches_no_changed_file_is_an_error_not_a_clean_result`,
  `excluding_every_changed_file_is_also_an_error`,
  `scope_binds_into_the_input_identity`,
  `oversized_diff_still_fails_closed_without_a_scope`.
- `crates/ripr/tests/cli_smoke.rs`:
  `check_scope_flags_turn_an_oversized_diff_into_a_disclosed_partial_result`.
- `crates/ripr/src/analysis/language/rust/mod.rs` tests:
  `committed_root_target_is_build_output_but_a_nested_target_module_is_not`.

## Implementation Mapping

- `crates/ripr/src/analysis/diff_scope_filter.rs`: `DiffScopeFilter`.
- `crates/ripr/src/analysis/pipeline.rs`: intake partition,
  `scope_matched_nothing_error`, `out_of_scope_limitation`.
- `crates/ripr/src/cli/commands/check.rs`: `--path`, `--exclude`.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
