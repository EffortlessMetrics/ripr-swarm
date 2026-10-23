# RIPR-SPEC-0347: Changed trybuild fixtures are named as not analyzed

Status: proposed

Owner: product / analysis

Created: 2026-10-23

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8999

Linked PRs:

- None yet

Support-tier impact:

- No tier change. This adds one advisory stderr line; it adds no class, oracle
  kind, relation reason, limitation kind or output field.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No schema change. The line is stderr text beside the existing unanalyzed-file
  disclosures; no JSON, SARIF, LSP or MCP output carries it.

## Problem

A diff can add or edit `trybuild` fixtures: `tests/ui/*.rs` files plus their
`.stderr` snapshots, driven by one `trybuild::TestCases` runner test. They are
compile-time expectations (the compiler output must match, or a pass file must
build and run), not `#[test]` functions with assertions ripr extracts. ripr
credited no oracle from them and said nothing, so a reader could take the
silence for "reviewed and clean".

## Behavior

1. A changed file is a trybuild fixture when its path has a `tests/ui` component
   pair with at least one further component below it, its extension is `rs`,
   `stderr` or `stdout`, and the manifest of the crate that owns the `tests`
   directory (the path prefix before it; the repository root for a bare
   `tests/ui/..`) declares `trybuild` in `[dependencies]`,
   `[dev-dependencies]`, `[build-dependencies]` or a `[target.*]` form of them. An explicit `package`
   key decides: `foo = { package = "trybuild" }` counts and
   `trybuild = { package = "other" }` does not. An inherited
   `workspace = true` entry is judged by its local key only; a rename in the
   workspace table is not resolved.
   A path alone is not enough: a `tests/ui` directory in a crate that does not
   depend on trybuild is never named. A missing, oversized or unparseable
   manifest names nothing, and a path with more than one `tests/ui` pair is
   ambiguous and names nothing. Only files in the diff intake's changed-file
   list are considered: a deleted-file section is omitted by intake and a pure
   rename supplies only its new path, so deleted fixtures are not disclosed.
   Known under-naming limits: a rename declared only in `[workspace.dependencies]`
   and inherited under another local key, and the underscore spelling
   `dev_dependencies`, are not detected (the disclosure only ever under-names).
2. When a diff changes at least one such fixture, `ripr check` writes one stderr
   line: `ripr: N changed trybuild fixture file(s) under tests/ui were not
   analyzed as test oracles: <first three paths> and K more. ...`. It states
   that the fixtures are compile-time expectations, add no oracle evidence and
   change no finding.
3. The line is advisory text only. It changes no finding, class, stop reason,
   limitation, analysis outcome kind, completeness, gate result or exit code,
   and no machine-readable output. The report for a diff is byte-identical
   whether or not the crate depends on trybuild, apart from the root path
   (compared as raw stdout bytes after replacing the scratch root).
4. Fixture files are not treated as unanalyzable in any other sense: the line
   neither suppresses a finding on the code the fixtures exercise nor credits
   the runner test.

## Required Evidence

- Positive: a root crate and a member crate that declare `trybuild` name their
  changed `tests/ui` `.rs` and `.stderr` files, bounded to three paths.
- Negative controls: `tests/ui` in a crate without trybuild, a manifest that
  only mentions `trybuild` outside a dependency table, a missing or unparseable
  manifest, and paths such as `tests/ui.rs`, `tests/other/ui/a.rs`,
  `src/tests/ui/a.rs` and non-fixture extensions name nothing.
- Invariance: the JSON report for one diff is identical with and without the
  trybuild dependency; only stderr differs.

## Non-Goals

- Reading `.stderr` expectations or pass-file `fn main` bodies as oracles.
- A typed limitation, a new output field or a human-report section.
- Other compile-test harnesses (`ui_test`, `compiletest`) or fixture
  directories other than `tests/ui`.

## Acceptance Examples

1. A diff adds `tests/ui/fail_len.rs` and `tests/ui/fail_len.stderr` to a crate
   with `trybuild` in `[dev-dependencies]`: stderr names both files as not
   analyzed as test oracles; the JSON report is unchanged.
2. The same diff in a crate without `trybuild`: no such line.

## Test Mapping

- `crates/ripr/src/analysis/trybuild_fixtures.rs` tests:
  `trybuild_crate_fixtures_are_named_with_their_paths`,
  `member_crate_manifest_owns_its_fixtures`,
  `ui_directory_in_a_crate_without_trybuild_is_not_named`,
  `missing_or_unparseable_manifest_names_nothing`,
  `target_specific_and_build_dependency_tables_count`,
  `trybuild_named_only_outside_dependency_tables_does_not_count`,
  `only_fixture_files_below_tests_ui_count`,
  `long_fixture_lists_are_bounded`,
  `nested_crate_path_uses_the_directory_owning_tests_ui`,
  `ambiguous_multiple_tests_ui_pairs_fail_closed`,
  `oversized_manifest_fails_closed`,
  `dependency_package_key_decides_whether_an_entry_is_trybuild`,
  `helper_names_any_changed_path_it_is_given_without_reading_the_fixture`.
- `crates/ripr/tests/trybuild_fixture_disclosure.rs`:
  `trybuild_fixture_diff_is_named_on_stderr_without_changing_the_report`.

## Implementation Mapping

- `crates/ripr/src/analysis/trybuild_fixtures.rs`:
  `trybuild_fixture_disclosure_message`.
- `crates/ripr/src/analysis/pipeline.rs`: stderr emission beside the
  unanalyzed-file disclosures.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
- Disclosing deleted fixture files or the old path of a rename: diff intake does not
  supply them, and changing intake is out of scope here.
