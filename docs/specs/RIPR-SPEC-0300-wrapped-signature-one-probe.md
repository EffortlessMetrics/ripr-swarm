# RIPR-SPEC-0300: A wrapped function signature seeds one probe, not one per physical line

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

- #8999 (round two: one probe per physical signature line)

Linked PRs:

- #9133

Support-tier impact:

- No tier change. This is probe-seeding hygiene: no new class, oracle kind,
  relation reason or limitation kind.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No schema version bump. No new wire shape.

## Problem

A function header wrapped over several physical lines (parameters, return
type, `where`, opening brace) produced one `static_unknown` probe per line, for
example 47 probes for one new function. A signature has no runtime behavior of
its own, so the per-line probes only repeated the body findings or split one
declaration change into several.

## Behavior

The signature extent is the physical lines from the `fn` line through the
single line whose masked text ends in the body-opening `{`. It is claimed only
when unambiguous: a body-less declaration (`;`), a one-line body, or a header
with any other `{` yields no extent and every line keeps its own probe.
Comment and string text is masked before this reading.

1. New function: when the `fn` line is an added line, no removed line lies in
   the function or declares the same name, and an added body line exists, the
   signature lines seed no probe.
2. Changed signature: when several added lines of one signature change, the
   first emitted probe stays on the first changed line (its id and span are
   unchanged); later added lines of that signature seed none, and their new
   text and old counterpart are appended (space-separated) to the retained
   probe's `after` and `before` so no edited parameter loses its evidence.
   Grouping reads a probe actually emitted for an earlier line, never an
   assumed one.
3. Any header line that touches a type bound (`where`, a bound predicate,
   `<T: Clone>`, `impl Trait`), even when it shares the line with parameters
   or the body opener, is exempt from both rules and keeps its own probe. An
   unparseable file treats every line as bound (no grouping).
4. A parameter inserted into an existing wrapped signature (the `fn` line is
   not added) is a changed signature and keeps its probe.

## Required Evidence

- New wrapped function: no probe on parameter, return or brace lines; the
  bound and body probes remain.
- Two changed parameter lines: one probe, on the first.
- Changed signature line plus changed body line: both probes remain.
- Inserted parameter, dropped carrier line, one-line body, trait declaration,
  braces in the header and trailing comments fail closed as above.

## Non-Goals

- Grouping removed-only signature lines.
- Changing type-bound declaration probes.

## Acceptance Examples

1. A new `pub fn price(\n amount: u64,\n years: u32,\n) -> u64\nwhere\n u64: Copy,\n{ ... }`
   with an added body: no probe on the parameter, return-type or brace lines;
   the `where` clause, its bound and the body keep theirs.
2. An existing wrapped signature with two parameter types changed: one probe,
   on the first changed line.
3. An existing wrapped signature with one parameter line inserted: that line
   keeps its probe.
4. `pub fn f(a: u8) -> u8 { a + 1 }` and a trait `fn f(\n a: u8,\n) -> u8;`
   keep per-line probes (no claimed extent).

## Test Mapping

- `crates/ripr/src/analysis/probes/diff.rs` tests:
  `probes_for_file_skips_wrapped_signature_lines_of_new_function_but_keeps_bound`,
  `probes_for_file_groups_changed_wrapped_signature_lines_into_one_probe`,
  `probes_for_file_keeps_body_probe_beside_changed_wrapped_signature`,
  `probes_for_file_keeps_parameter_inserted_into_existing_wrapped_signature`,
  `probes_for_file_keeps_probe_when_earlier_signature_line_emitted_none`,
  `signature_open_line_fails_closed_on_ambiguous_headers`.
- `crates/ripr/src/analysis/probes/where_bound_boundary_tests.rs`:
  `new_function_diff_keeps_bounds_unknown_and_body_executable`.

## Implementation Mapping

- `crates/ripr/src/analysis/probes/diff.rs`: `signature_open_line`,
  `opens_new_function_with_added_body`, `continues_changed_signature`,
  `is_signature_type_bound`.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
