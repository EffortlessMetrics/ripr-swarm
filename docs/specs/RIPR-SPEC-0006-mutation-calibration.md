# RIPR-SPEC-0006: Mutation Calibration Reports

Status: proposed

Lifecycle review: 2026-08-25. Retained as proposed; acceptance remains blocked
on the runtime/static join cases and advisory agreement summary contract.

## Problem

`ripr` gives fast static seam evidence. Real mutation execution can later confirm
or correct those static predictions, but that runtime evidence currently has no
standard place to land.

Without a calibration report, agents and maintainers cannot compare
`SeamGripClass` predictions with cargo-mutants outcomes in a repeatable way, and
runtime mutation vocabulary can leak into static reports where it would overclaim
what `ripr` has proven.

## Behavior

`ripr` should provide an advisory calibration report that joins static seam
evidence to imported cargo-mutants JSON/output.

The report should:

- read current repo seam exposure evidence;
- import runtime mutation records from a supplied JSON file or `mutants.out`
  directory;
- combine `mutants.out/outcomes.json` with `mutants.out/mutants.json` when both
  are available;
- import span-based cargo-mutants locations when generated mutant records carry
  a `span` object instead of a flat `line` field;
- retain the complete cargo-mutants source span of each runtime record (see
  Runtime Source Spans);
- join records by `seam_id` when present;
- otherwise join by span containment when the runtime record and static seams
  carry complete spans (see Join Precedence);
- fall back to normalized file + line matching only where span comparison is
  unavailable;
- report file/line fallback matches as ambiguous when multiple candidates
  share the same normalized file and line;
- report equal or crossing containing spans as ambiguous span overlaps;
- report unmatched runtime mutants separately, with the reason they did not
  join;
- summarize static/runtime agreement in advisory buckets;
- label imported static/runtime agreement as confidence context without changing
  static classifications;
- preserve samples of runtime gap signals without static gaps;
- preserve samples of static gap seams without runtime gap signals;
- keep static seam fields and runtime mutation fields separate;
- write `target/ripr/reports/mutation-calibration.json`;
- write `target/ripr/reports/mutation-calibration.md`;
- stay advisory and non-blocking by default.

Runtime mutation outcome words are allowed only in this calibration/runtime
report family. Static check, exposure, badge, context, and editor reports must
continue using the audit vocabulary.

## Required Evidence

Each matched calibration row should carry:

- `seam_id`
- `seam_kind`
- `seam_grip_class`
- oracle kind and strength
- observed values
- missing discriminators
- mutation operator
- runtime outcome
- duration, when provided by the runtime data
- test command, when provided by the runtime data
- join method (`seam_id`, `span_containment` or `file_line`)
- confidence label, one of:
  - `supports_static_gap`
  - `contradicts_static_gap`
  - `supports_static_clean`
  - `contradicts_static_clean`
  - `no_runtime_data`

Ambiguous file/line matches should keep the runtime record and list all static
candidate seams without assigning the runtime outcome to any single seam. These
rows should carry `ambiguous_runtime_join` so consumers know not to raise
confidence from that runtime record.

Ambiguous span overlaps follow the same rule. Both ambiguity lists carry the
runtime span and every candidate seam with its span, so the decision can be
reproduced from the report.

Unmatched runtime mutants should preserve their location, mutation operator,
runtime outcome, duration, and test command when available, plus an
`unmatched_reason`:

- `no_location`: the runtime record names no file or no line;
- `conflicting_runtime_spans`: merged duplicates of the record disagreed on
  span or line, so its location is untrusted;
- `no_seam_on_line`: no static seam starts on the record's line and no seam
  span in the file could be compared;
- `no_containing_seam`: the record has a complete span, the file has seam
  spans, none contains the mutated range, and no span-less seam starts on the
  record's line. These rows also carry `line_seams`: the seams starting on
  the record's line, with their spans.

## Runtime Source Spans

cargo-mutants records each mutant's location as `span.start` / `span.end`
with 1-based line and character columns and an exclusive end. The importer
keeps that range as one atomic value:

- the span is present only when `start.line`, `start.column`, `end.line` and
  `end.column` are all present, columns are at least 1, `start <= end`, and
  `start.line` agrees with the record's `line`;
- a partial, zero, inverted or inconsistent span is dropped whole and the
  record stays line-only evidence; coordinates are never synthesized or mixed
  between sources;
- `function.span` is never the mutant's span;
- when `mutants.json` and `outcomes.json` records for one mutant merge, a
  complete span fills an absent one; two different complete spans drop the
  span and mark the record `span_status: "conflicting_runtime_spans"` rather
  than choosing one, as does a duplicate that disagrees on the line; such a
  record joins only by `seam_id`;
- the span is rendered on runtime rows as `column`, `end_line` and
  `end_column` next to `line`.

## Join Precedence

1. **Explicit `seam_id`.** A runtime `seam_id` naming a static seam joins it.
2. **Span containment.** When the runtime record has a complete span, every
   static seam in the same normalized file with a complete span
   (`repo-exposure-json` 0.4 `column`, `end_line`, `end_column`) is compared,
   whatever line the seam starts on. Positions compare lexicographically as
   `(line, column)` with exclusive ends; a seam contains the mutant when
   `seam.start <= mutant.start`, `mutant.start < seam.end` and
   `mutant.end <= seam.end`.
   - Exactly one containing seam, or a unique innermost one (strictly inside
     every other containing seam), joins with `join_method =
     span_containment`.
   - Equal or crossing innermost spans are a true overlap: the record goes to
     `ambiguous_span_overlap_matches` with every innermost candidate. Source
     order, seam order, span length and ID never break the tie.
   - Span-less seams do not compete with a containment match and are never
     reported as span-precise.
3. **File and line fallback.** With no containing span, span-less seams on the
   record's start line join by `file_line` (unique) or go to
   `ambiguous_file_line_matches` (several). A record without a complete span
   uses this fallback over every seam on its line, as before spans existed.
   A seam span that disproves containment is never rescued by sharing the
   line.

Candidate lists are ordered by seam line, column, end line, end column and
seam ID, independent of snapshot order.

`SeamId` generation does not change: geometry is join evidence, not identity.

Runtime gap signals that cannot be joined to a static seam should carry
`runtime_only_signal`; they are calibration context only and must not create a
static gap.

The agreement summary should count:

- static gap seams with a matched runtime gap signal;
- static gap seams without a matched runtime gap signal;
- runtime gap signals without a matching static gap;
- static-clean seams with runtime-clean labels;
- inconclusive runtime labels that should not be counted as agreement.

## Non-Goals

This spec does not require:

- running cargo-mutants;
- blocking CI;
- changing static seam classifications;
- recalibrating classification thresholds automatically;
- SARIF output;
- global suite scoring;
- adding runtime mutation vocabulary to static reports.

## Acceptance Examples

### Runtime mutant matches by seam ID

```text
Given a repo exposure seam with seam_id = abc123,
and imported cargo-mutants JSON has a runtime record with seam_id = abc123,
when ripr calibrate cargo-mutants runs,
then the report emits one matched row with join_method = seam_id.
```

### Runtime mutant matches by file and line

```text
Given a repo exposure seam at src/pricing.rs:42,
and imported cargo-mutants JSON has no seam_id but has file = src/pricing.rs
and line = 42,
when ripr calibrate cargo-mutants runs,
then the report emits one matched row with join_method = file_line.
```

### Unmatched runtime mutant remains visible

```text
Given imported runtime data for src/other.rs:99,
and no static seam matches that seam_id or file/line,
when ripr calibrate cargo-mutants runs,
then the report lists the runtime mutant under unmatched_mutants.
```

### Span containment picks the seam holding the mutated token

```text
Given a predicate seam at src/gate.rs:10:12-10:25 nested in a return seam
at src/gate.rs:10:5-10:40,
and a cargo-mutants record replacing `&&` at src/gate.rs:10:18-10:20,
when ripr calibrate cargo-mutants runs,
then the report joins the record to the predicate seam with
join_method = span_containment.
```

### A same-line seam that does not contain the mutant is not joined

```text
Given a call seam around `digits(self.minor)` at src/display.rs:20:19-20:37,
and cargo-mutants records replacing the `+` at src/display.rs:20:17-20:18,
when ripr calibrate cargo-mutants runs,
then the records are listed under unmatched_mutants with
unmatched_reason = no_containing_seam.
```

### Equal spans stay a true overlap

```text
Given a predicate seam and a return seam that both span src/context.rs:40:5-40:47,
and a cargo-mutants record inside that range,
when ripr calibrate cargo-mutants runs,
then the report lists the record under ambiguous_span_overlap_matches with
both candidates and their spans, and joins neither.
```

### Ambiguous file and line match stays unassigned

```text
Given two repo exposure seams at src/pricing.rs:42,
and imported cargo-mutants JSON has no seam_id but has file = src/pricing.rs
and line = 42,
when ripr calibrate cargo-mutants runs,
then the report lists the runtime mutant under ambiguous_file_line_matches
and does not pick the first seam as a definitive match.
```

### Agreement summary stays advisory

```text
Given matched runtime data with static gaps, static-clean seams, runtime gap
signals, runtime-clean labels, and runtime-inconclusive labels,
when ripr calibrate cargo-mutants runs,
then the report emits agreement counts, precision notes, static-only finding
samples, and missed-runtime-signal samples without changing static seam classes.
```

### Confidence labels stay advisory

```text
Given static gap seams, static-clean seams, runtime gap labels, runtime-clean
labels, ambiguous file/line joins, unmatched runtime gap signals, and seams with
no usable runtime signal,
when ripr calibrate cargo-mutants runs,
then matched/sample rows include static/runtime confidence labels and those
labels do not change static seam classes or gate behavior.
```

## Test Mapping

Current tests:

- `crates/ripr/src/output/mutation_calibration.rs::tests::mutation_calibration_summarizes_static_runtime_agreement`
- `crates/ripr/src/output/mutation_calibration.rs::tests::mutation_calibration_joins_by_seam_id_then_file_line_and_keeps_ambiguous`
- `crates/ripr/src/output/mutation_calibration.rs::tests::mutation_calibration_parses_repo_exposure_and_cargo_mutants_json`
- `crates/ripr/src/output/mutation_calibration.rs::tests::mutation_calibration_merges_mutants_and_outcomes_by_id`
- `crates/ripr/src/output/mutation_calibration.rs::tests::mutation_calibration_reports_are_advisory_and_structured`
- `crates/ripr/src/output/mutation_calibration.rs::tests::span_join_refuses_a_same_line_seam_that_does_not_contain_the_mutant`
- `crates/ripr/src/output/mutation_calibration.rs::tests::span_join_scores_multi_seam_lines_on_the_innermost_containing_seam`
- `crates/ripr/src/output/mutation_calibration.rs::tests::span_join_reports_equal_and_crossing_overlaps_and_multiline_containment`
- `crates/ripr/src/output/mutation_calibration.rs::tests::span_join_ranks_containment_over_line_fallback_and_seam_id_over_both`
- `crates/ripr/src/output/mutation_calibration.rs::tests::merging_runtime_records_keeps_complete_spans_and_refuses_conflicts`
- `crates/ripr/src/output/mutation_calibration.rs::tests::span_join_reads_cargo_mutants_columns_end_to_end`
- `crates/ripr/src/output/mutation_calibration.rs::tests::untrusted_runtime_locations_stay_unmatched`
- `crates/ripr/src/output/mutation_calibration.rs::tests::id_less_runtime_records_sort_independently_of_input_order`
- `crates/ripr/src/output/mutation_calibration/outcome_records.rs::tests::reads_mutant_span_columns_and_drops_malformed_ends`
- `crates/ripr/src/cli/commands.rs::tests::calibrate_parses_required_inputs_format_and_out`
- `crates/ripr/src/cli/commands.rs::tests::calibrate_command_writes_json_file`
- `crates/ripr/tests/cli_smoke.rs::calibrate_cargo_mutants_prints_markdown_by_default`
- `crates/ripr/tests/cli_smoke.rs::calibrate_cargo_mutants_writes_json_when_requested`
- `crates/ripr/tests/cli_smoke.rs::calibration_runtime_fixture_matches_checked_reports`
- `crates/ripr/tests/cli_smoke.rs::calibration_runtime_fixture_v2_matches_checked_reports`
- `crates/ripr/tests/cli_smoke.rs::calibration_runtime_fixture_v3_matches_checked_reports`
- `crates/ripr/tests/cli_smoke.rs::calibration_span_containment_atuin_fixture_matches_checked_reports`
- `crates/ripr/tests/cli_smoke.rs::calibration_span_containment_semver_display_refuses_same_line_pairing`
- `xtask/src/main.rs::mutation_calibration_args_parse_root_and_input_paths`
- `xtask/src/main.rs::mutation_calibration_imports_static_seams_and_runtime_outcomes`
- `xtask/src/main.rs::mutation_calibration_merges_mutants_and_outcomes_by_mutant_id`
- `xtask/src/main.rs::mutation_calibration_imports_span_based_mutant_locations`
- `xtask/src/main.rs::mutation_calibration_directory_input_combines_outcomes_and_mutants`
- `xtask/src/main.rs::mutation_calibration_joins_by_seam_id_then_file_line`
- `xtask/src/main.rs::mutation_calibration_summarizes_static_runtime_agreement`
- `xtask/src/main.rs::mutation_calibration_reports_ambiguous_file_line_without_selecting_first`
- `xtask/src/main.rs::mutation_calibration_uses_same_static_without_runtime_sample_limit_for_json_and_markdown`
- `xtask/src/main.rs::mutation_calibration_reports_are_advisory_and_structured`

Checked fixture-backed samples:

- `fixtures/boundary_gap/calibration/runtime-fixtures-v1/` covers the main
  static/runtime agreement buckets, ambiguous file/line joins, unmatched runtime
  mutants, static seams without runtime data, and both `seam_id` and
  unambiguous `file_line` joins.
- `fixtures/boundary_gap/calibration/runtime-fixtures-v2/` covers checked
  observer-class runtime imports for side-effect observers, mock expectations,
  snapshot oracles, and opaque dispatch. The sample maps runtime outcomes to
  existing static seams where possible, keeps ambiguous file/line opaque
  dispatch joins ambiguous, and keeps a runtime-only signal out of static gap
  creation.
- `fixtures/boundary_gap/calibration/runtime-fixtures-v3/` covers checked
  static/runtime confidence expansion imports for custom assertion helpers,
  table-driven boundaries, builder overrides, cross-file constants, snapshot
  field discriminators, and mock expectation mismatches. The sample maps
  runtime outcomes to existing static seams where possible, keeps ambiguous
  file/line joins ambiguous, keeps runtime-only signals out of static gap
  creation, and preserves `no_runtime_data` for a checked static gap without
  runtime data.

- `fixtures/boundary_gap/calibration/span-containment-atuin/` pins 27 real
  cargo-mutants outcomes from Atuin `90f590b9` against reduced seam metadata:
  8 `span_containment` joins, 5 `file_line` fallbacks, 6 equal-span overlaps,
  8 `no_containing_seam` records, and none of the 16 line-only ambiguities
  left in `ambiguous_file_line`.
- `fixtures/boundary_gap/calibration/span-containment-semver-display/` pins
  the semver `display.rs:20` wrong same-line pairing as `no_containing_seam`.

Planned tests:

- end-to-end smoke around a real cargo-mutants output artifact when runtime cost
  is acceptable.

## Implementation Mapping

Current implementation:

- `crates/ripr/src/cli/commands.rs` implements
  `ripr calibrate cargo-mutants`.
- `crates/ripr/src/output/mutation_calibration.rs` parses repo exposure JSON
  and imported cargo-mutants JSON.
- `ripr calibrate cargo-mutants` accepts either a JSON file path or a
  cargo-mutants output directory containing `outcomes.json` or `mutants.json`.
- `ripr calibrate cargo-mutants` renders Markdown by default and supports
  `--format json` plus `--out`.
- `xtask/src/main.rs` keeps repo-local `cargo xtask mutation-calibration`
  automation for generated reports under `target/ripr/reports/`.

The public command is an installed-binary adoption surface. It remains
advisory and does not make calibration a public library API.

## Metrics

- `static_seams_total`
- `mutants_total`
- `matched_total`
- `ambiguous_file_line_total`
- `ambiguous_span_overlap_total`
- `unmatched_mutants_total`
- unmatched reason counts
- `static_without_runtime_total`
- `static_gap_and_runtime_signal`
- `static_gap_without_runtime_signal`
- `runtime_signal_without_static_gap`
- `static_clean_and_runtime_clean`
- `runtime_inconclusive`
- static/runtime confidence labels
- runtime outcome counts
- join method counts
