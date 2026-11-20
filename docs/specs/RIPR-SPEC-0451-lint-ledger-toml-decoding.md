# RIPR-SPEC-0451: Lint ledger TOML decoding

Status: accepted

Owner: xtask / repository policy

Linked issue: #13217.

## Problem

`policy/clippy-lints.toml` records active and planned Rust and Clippy lint
levels. A line-oriented reader can lose a planned row that sits under a
commented header, mis-read alternate table spellings or multiline strings, and
let a malformed known field silently become absent. A lost row also hides an
overdue MSRV obligation or a collision with a debt entry.

## Behavior

`parse_clippy_lints_ledger` decodes the ledger as TOML and returns entries in
source order together with violation strings; it never panics on input.

- A document that does not parse as TOML yields no entries and one violation.
  A repeated key is a parse error.
- Known fields keep their declared types. A wrong-typed known field is a parse
  violation, not an absent field. Unknown fields remain documentary extensions.
- A commented table header does not create a row, and a commented planned row
  is not a row; a row written in any valid TOML table or inline-array form is
  retained with the one-based line of its first byte.
- A row with an empty `name`, a name that is not a bare lint name or
  `clippy::<name>`, an empty or unsupported `level`, or an identity already seen
  in any family or state yields a violation and no entry.
- Bare Rust names and `clippy::`-prefixed names are distinct identities; a debt
  entry or planned row for one does not cover the other.

Consumers (`collect_lint_policy_violations`, `collect_clippy_debt_violations`)
use the parser output unchanged and retain its parse errors.

## Required Evidence

- Planned rows written with a commented header or as an inline array keep the
  overdue-MSRV obligation.
- Wrong-typed known fields, invalid MSRV strings, malformed TOML and duplicate
  keys are rejected or reach the existing version check.
- Duplicate identities across families and states are rejected.
- Documentary extension fields and multiline non-MSRV blockers remain valid.
- Source order, per-form line numbers and the Rust/Clippy namespace split are
  preserved.
- The checked-in ledger parses with valid unique rows.

## Non-Goals

- Deciding which lints should be active, or their levels.
- Compiling or running Clippy; this is a policy read of the ledger only.
- Changing the debt ledger schema.

## Acceptance Examples

1. A planned row under a commented header with an overdue `activate_when_msrv`
   still produces the MSRV violation.
2. `name = "clippy::x"` and `name = "x"` are separate identities; the same
   identity twice is a violation.
3. A numeric `level` or a malformed document produces a violation rather than a
   silently empty ledger.

## Test Mapping

All tests are in `xtask/src/policy/lint_ledger.rs` under `tests`:

- Example 1: `commented_planned_header_keeps_overdue_msrv_obligation`,
  `inline_planned_array_keeps_overdue_msrv_obligation`,
  `commented_planned_row_still_collides_with_debt`.
- Example 2: `duplicate_identities_across_families_and_states_fail`,
  `equal_bare_names_require_both_namespace_receipts`.
- Example 3: `known_field_types_cannot_silently_become_absent`,
  `malformed_and_duplicate_key_toml_fail_closed`,
  `invalid_msrv_strings_reach_the_existing_version_check`,
  `invalid_identities_and_levels_do_not_create_planned_entries`.
- Compatibility: `documentary_extension_fields_remain_supported`,
  `multiline_non_msrv_blocker_remains_valid`,
  `source_order_and_distinct_namespaces_are_preserved`,
  `source_spans_report_entry_lines_for_each_toml_form`,
  `debt_consumer_retains_lint_ledger_parse_errors`,
  `repository_ledger_has_valid_unique_rows`.

The traceability fragment registers these symbols; a traceability pass does not
prove they ran.

## Implementation Mapping

- `xtask/src/policy/lint_ledger.rs::parse_clippy_lints_ledger` owns decoding.
- `xtask/src/policy_root.rs` and `xtask/src/main.rs` consume the parsed entries.
- `docs/CLIPPY_POLICY.md` describes the ledger contract.

## Metrics

- `unit_test_pass_rate` for the named tests.
