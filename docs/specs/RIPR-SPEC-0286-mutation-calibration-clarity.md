# RIPR-SPEC-0286: Mutation calibration output clarity contract

Status: proposed

Owner: product / output

Created: 2026-10-20

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #8750 (the calibration agreement table ships no vocabulary: a saved
  report does not define static/runtime gaps, clean buckets, or what a
  disagreement row means)
- #8753 (the head-identity disclosure is stderr-only, so a report saved
  with `--out` loses the "same subject?" caveat that keeps a coincidental
  file:line join honest)

Linked PRs:

- None yet

Support-tier impact:

- None. The report gains in-band provenance and a reading guide it always
  owed its readers; no command accepts or executes anything new.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- The calibration JSON schema version is bumped `0.2` → `0.3`
  (additive: top-level `provenance` and `how_to_read` keys; no existing
  key changes meaning).
- No new exposure class, oracle kind, severity, or grip-class value. The
  provenance verdict strings (`same_head`, `different_heads`,
  `only_repo_exposure_head`, `only_mutants_head`, `no_head_recorded`)
  describe artifact identity only; they never change a join or a
  classification.
- Behavior ownership stays with RIPR-SPEC-0006 (the calibration report
  and its join); this spec records the output-clarity contract that
  #8750 and #8753 created on top of it.

## Problem

`ripr calibrate cargo-mutants` renders an advisory report joining static
seam evidence to imported cargo-mutants outcomes. Two defects made a
saved report teach the wrong lesson:

1. The agreement table used vocabulary the report never defined. A
   `static_gap_and_runtime_signal` count means "the two sources agree"
   only if the reader already knows what a static gap is (a seam outside
   the strong set `strongly_gripped` / `intentional` / `suppressed`),
   what a runtime gap signal is (`missed` / `survived` / `not_caught` /
   `uncaught`), and that `runtime_signal_without_static_gap` rows are the
   first to investigate. A report saved with `--out` and read later, or
   read by someone who did not run the command, carried no way to learn
   that vocabulary from the artifact itself.
2. The head-identity caveat — "these two artifacts may describe
   different repository subjects" — was printed only on stderr. A report
   saved with `--out` silently inherited the authority of whatever
   snapshot it was joined against, including the high-signal confidence
   labels a coincidental file:line join can produce (#7449). The archived
   surface contradicted the interactive one: the caveat existed exactly
   where a reader could no longer see it.

## Behavior

### Schema 0.3 provenance (#8753)

The calibration report schema version is `0.3`. The report carries a
`CalibrationProvenance` value, populated in
`mutation_calibration_report_from_json` from both input artifacts via
`snapshot_repository_head`:

- `repo_exposure_head`: the repo-exposure snapshot's repository head, or
  null when the snapshot does not carry one (or carries a placeholder,
  using the same `is_full_sha` rule as `ripr outcome`).
- `mutants_head`: the cargo-mutants artifact's repository head, or null —
  the common case, since `outcomes.json` does not carry a head SHA.
- `verdict`: one of `same_head`, `different_heads`,
  `only_repo_exposure_head`, `only_mutants_head`, `no_head_recorded`,
  derived from the two optional heads.

The verdict is evidence about the artifacts, not about the join: a
`different_heads` verdict never refuses, rewrites, or re-labels a match.
The existing disclosure-sentence tests for the stderr surface stay the
owner of the interactive wording; `disclosure_sentence()` mirrors the
same facts so the md and JSON surfaces cannot drift from it.

### Provenance on both rendered surfaces

The md renderer gains a `## Provenance` section after the preamble and
before `## Summary`, rendering the disclosure sentence plus the standing
note that the join itself matches by seam_id, span containment, or
file/line in every case. The JSON renderer gains a top-level
`provenance` object with the same three fields and verdict. Both are
rendered for every report, including one built without artifact inputs
(`no_head_recorded`), so a saved artifact always discloses what it can
and cannot claim about subject identity.

### How-to-read on both rendered surfaces (#8750)

A shared `agreement_how_to_read()` map — one non-empty guide line per
agreement bucket, keyed by the bucket name — drives both renderers, so
the md bullet list and the JSON `how_to_read.buckets` object cannot
drift apart.

- The md `## Static/runtime agreement` section opens with a "How to read
  this table:" paragraph defining a **static gap** (a seam whose grip
  class is outside the strong set `strongly_gripped` / `intentional` /
  `suppressed`), a **runtime gap signal** (`missed`, `survived`,
  `not_caught`, `uncaught`), and a **runtime clean signal** (`caught`,
  `timeout`), followed by one `- `<bucket>`:` guide line per bucket.
- The JSON report gains a top-level `how_to_read` object defining
  `static_gap`, `runtime_gap_signal`, and `runtime_clean_signal`, plus a
  `buckets` object whose values are verbatim the md guide lines.

The guide interprets; it does not reclassify. Bucket counts, confidence
labels, and join behavior are unchanged.

## Required Evidence

- The provenance verdict maps all five head combinations, including both
  null (`no_head_recorded`) and equal heads (`same_head`).
- `disclosure_sentence()` is non-empty for every verdict and names every
  head the artifacts actually carry.
- `CalibrationProvenance::json()` carries both head fields (null when
  absent) and the verdict string.
- The md report renders `## Provenance` between the preamble and
  `## Summary`, and the JSON report carries a top-level `provenance`
  object; both appear even when no artifact records a head.
- `agreement_how_to_read()` has exactly five entries, one per agreement
  bucket, all non-empty; the md renders the "How to read this table:"
  block and one guide line per bucket; the JSON `how_to_read` object
  defines the three vocabulary terms and carries the same five guide
  lines verbatim.
- The checked calibration fixtures regenerate byte-exactly from the real
  binary on both md and JSON surfaces.

## Test Mapping

- `crates/ripr/src/output/mutation_calibration.rs::tests::calibration_provenance_verdict_covers_all_head_combinations`
- `crates/ripr/src/output/mutation_calibration.rs::tests::calibration_provenance_disclosure_sentence_is_nonempty_and_names_verdict_facts`
- `crates/ripr/src/output/mutation_calibration.rs::tests::calibration_provenance_json_shape_mirrors_verdict`
- `crates/ripr/src/output/mutation_calibration.rs::tests::agreement_how_to_read_is_present_on_md_and_json_surfaces`
- `crates/ripr/src/output/mutation_calibration.rs::tests::mutation_calibration_summarizes_static_runtime_agreement`
  (schema version pin, now `0.3`)
- `crates/ripr/src/output/mutation_calibration.rs::tests::span_join_reads_cargo_mutants_columns_end_to_end`
  (schema version pin, now `0.3`)
- `crates/ripr/tests/cli_smoke.rs::calibration_runtime_fixture_v1_matches_checked_reports`
- `crates/ripr/tests/cli_smoke.rs::calibration_runtime_fixture_v2_matches_checked_reports`
- `crates/ripr/tests/cli_smoke.rs::calibration_runtime_fixture_v3_matches_checked_reports`
- `crates/ripr/tests/cli_smoke.rs::calibration_span_containment_atuin_fixture_matches_checked_reports`
- `crates/ripr/tests/cli_smoke.rs::calibration_span_containment_semver_display_refuses_same_line_pairing`

## Acceptance Examples

Provenance for the common case — a headed repo-exposure snapshot joined
to a headless cargo-mutants JSON:

```json
"provenance": {
  "repo_exposure_head": "2bd22c0b0718157870e2e78c9a70b9da1c9c1b21",
  "mutants_head": null,
  "verdict": "only_repo_exposure_head"
}
```

The md section for an import where neither artifact records a head:

```text
## Provenance

Neither artifact carries a repository head SHA, so ensure both artifacts
are from the same repository and adjacent commits. The join itself
matches by seam_id, span containment, or file/line in every case.
```

The agreement section now opens with the vocabulary it uses:

```text
How to read this table: a **static gap** is a static seam whose grip
class is outside the strong set `strongly_gripped` / `intentional` /
`suppressed` (for example `ungripped` or `weakly_gripped`). A **runtime
gap signal** is an imported cargo-mutants outcome the runtime failed to
catch (`missed`, `survived`, `not_caught`, `uncaught`); a **runtime
clean signal** is one it caught (`caught`, `timeout`).

- `runtime_inconclusive`: ambiguous joins ...
```

## Non-Goals

- No new exposure class, grip class, runtime-outcome vocabulary, or
  confidence label.
- The provenance verdict never refuses, rewrites, or re-labels a join; a
  coincidental file:line join still joins, with the same disclosure.
- The schema bump is additive; no existing key changes meaning and no
  key is removed.
- The interactive stderr disclosure keeps its existing wording and
  remains the primary interactive surface.

## Implementation Mapping

- `crates/ripr/src/output/mutation_calibration.rs` owns
  `CalibrationProvenance` (verdict, `disclosure_sentence`, `json`), the
  `provenance` field on `MutationCalibrationReport`, the schema literal
  `MUTATION_CALIBRATION_SCHEMA_VERSION`, the shared
  `agreement_how_to_read()` map and its JSON projection
  `agreement_how_to_read_json()`, and both renderers' new sections.
- `crates/ripr/src/output/outcome/mod.rs::snapshot_repository_head` owns
  head extraction; both input artifacts are read through it.
- `crates/ripr/src/cli/commands.rs` owns the stderr disclosure and its
  existing tests; `crates/ripr/src/cli/help/reports.rs` documents the
  Provenance section / provenance field in the calibrate help text.
- Checked fixtures under `fixtures/boundary_gap/calibration/` regenerate
  from the built binary (`ripr calibrate cargo-mutants --mutants-json
  ... --repo-exposure-json ... --format md|json --out ...`).

## Metrics

- `unit_test_pass_rate` on the registered tests.
