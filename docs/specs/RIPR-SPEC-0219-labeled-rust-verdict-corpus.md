# RIPR-SPEC-0219: Labeled Rust verdict corpus

Status: proposed

Owner: product-eval

Created: 2026-10-04

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- None yet

Linked PRs:

- None yet

Support-tier impact:

- None. The corpus measures verdict accuracy on named cases. It does not
  promote a language, editor surface, gate, or public support claim.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- Add `fixtures/rust-verdict-corpus` to the manifest-only fixture set.
- Retained upstream Rust files are stored as `<name>.rs.txt`, byte-identical
  to upstream, and get their `.rs` name back only in the run-owned copy. The
  vendored code is fixture data: it adds no workspace Rust, no lint or
  process-policy allowlist rows, and no lines to the PR diff-scope budget.
- No new crates, binaries, dependencies, network allowlist rows, process
  spawn sites, or support-tier changes. The harness reuses the fixture
  runner's ripr build and process owner.

## Problem

Trust in a ripr verdict has been argued from anecdotes: one serde loop that
ripr refused, one report that said "no static test path" beside 81 related
tests. Existing judged panels
([RIPR-SPEC-0092](RIPR-SPEC-0092-python-judged-pr-panel.md) for Python, the
Rust judged-behavior panel seed) fix the vocabulary of both error directions
but carry synthetic or unjudged Rust subjects. Nothing measures how often
ripr's Rust verdict is wrong on real tests whose behavior has been worked
out, so a fix to one verdict cannot show it did not break another.

## Behavior

`fixtures/rust-verdict-corpus/corpus.json` (`ripr_verdict_corpus.v1`) holds
subjects and cases.

A subject has an `origin`. An `upstream` subject (the default) is one
pinned upstream repository: URL, 40-hex commit, version label, license, an
optional reference into the shared Rust corpus manifest (`corpus_version`
`2026-10-04.1`), and the retained excerpt files under `subjects/<id>/` with
their sha256. Retained files are byte-identical to the upstream commit and
include its license files.

An `authored` subject is a small crate written for this corpus to fill a
verdict or probe-family cell the upstream cases leave empty. Its id starts
with `authored-` (an upstream id may not), it carries no upstream URL,
commit, or shared-corpus reference, its license is this repository's
(`MIT OR Apache-2.0`), it retains no license file of its own, and the whole
crate is retained. Its cases carry runtime truth exactly as upstream cases
do, with the whole stored crate standing in for the pinned checkout.
Authored cases are chosen to fill cells, so their rates are not real-world
rates: the report gives the false-verdict, false-actionable, false-exposed,
false-silent, ideal, and abstention rates again per origin under
`by_origin`, and a row names its origin.

A case is one edit in one subject: a unified diff under `cases/`, the
anchored file and line the diff adds, the edit kind, behavior family, test
shape, an optional hard-case note, the truth label, the expected verdicts,
written reasoning, and what ripr reported when the case was labeled.

Truth is runtime evidence. The edit was applied to the full pinned checkout
and the crate's own test command run. For a `behavior_preserving_rewrite`
the edit keeps tests green and each listed mutant of the edited expression
was applied on top; for a `behavior_change` the edit is its own single
mutant. A mutant counts as detected (`tests_failed`) when the test command
fails after compiling.
Each mutant carries a written equivalence review and the test that failed.
A mutant that is equivalent in the pinned build cannot carry truth: its
passing tests show nothing about a missing discriminator. A reported miss
that turns out equivalent is replaced by a non-equivalent mutant of the same
expression, or the line is left out. Truth runs in a build that compiles
the anchored line and enables the features its behavior depends on; a mutant
in code a feature gate leaves out is not evidence. Each case records the
toolchain and test command its truth ran under.
Truth derives from how many mutants failed the tests: all is `discriminated`, none is
`not_discriminated`, otherwise `partially_discriminated`.

Expected verdicts follow one table the validator enforces:

| Truth | Ideal | Acceptable |
| --- | --- | --- |
| discriminated | credited | credited, limited, silent |
| partially_discriminated | gap | gap, limited |
| not_discriminated | gap | gap, limited |

The observed verdict reads only candidate-current findings on the anchor:
`exposed` is credited; a named `static_limit_kind` or a `no_static_path`,
`infection_unknown`, `propagation_unknown`, or `static_unknown` class is
limited; any other class is a gap; no anchored finding is silent. When the
anchor is a changed single-line `let`, RIPR-SPEC-0157 moves ripr's probe to
the predicate that uses the binding, so the projection also reads
candidate-current findings in the anchor file whose evidence carries
``binding_predicate_relation: changed binding `<name>` `` for the binding the
anchor declares, with the anchor's initializer as the new initializer, and
the row records `followed_retarget`. The case keeps the line its diff adds as
its anchor. Only a plain `let name` (optionally `mut`) is followed; patterns
fail closed. This couples the projection to the wording of that evidence
line: a wording change stops the following, and a retargeting case then
reads silent until the projection is updated. A gap on
the line outranks credit, and credit outranks a limit. The per-finding
reading mirrors the human "Start here" triage. The line-level precedence is
this corpus's own policy, not triage's ranking: triage orders findings to
pick where to start, while the corpus asks which verdict a developer reading
the line would act on, and a gap there routes repair work.

Each case scores as `ideal`, `abstained` (acceptable but not ideal),
`false_actionable` (a gap where tests discriminate), `false_exposed` (credit
where they do not fully), or `false_silent` (no verdict where they do not
fully).

Contradictions are internal to ripr's output and need no label:
`reach_yes_without_related_tests`, `no_static_path_with_related_tests`,
`exposed_without_discriminator`, `related_tests_listed_exceed_total`, and
summary counts that disagree with the findings list.

`cargo xtask verdict-corpus` has three subcommands:

- `validate` checks the corpus offline: schema, subject digests and
  unlisted files, diff anchors, truth derived from mutant outcomes, and the
  label table.
- `report [--out <dir>]` copies each subject to a run-owned workspace under
  `target/ripr/verdict-corpus/`, applies the case diff with a strict patch
  reader that refuses drifted context, runs `ripr check --json`, and writes
  `report.json` and `report.md`.
- `check [--out <dir>]` does the same and fails when `report.json` differs
  from `fixtures/rust-verdict-corpus/expected/report.json` or `report.md`
differs from `expected/report.md`. It refuses an
  `--out` that is the expected directory, so it cannot replace its golden.

The report states false-verdict, false-actionable (over discriminated
cases), false-exposed and false-silent (over the rest), ideal, abstention,
and contradiction rates as exact fractions, the verdict rates again per
subject origin, and one row per case with its origin and a
`changed_since_labeling` flag.

The contradiction rate counts candidate-current findings across each case's
whole `ripr check` run, not only the anchor line, because a
self-contradicting finding anywhere is an internal inconsistency.
`contradictions_by_code` uses the same unit: findings carrying each code,
plus one per run for a summary-count code. A row's `contradictions` lists
the distinct codes seen in that case's run.

### Spec-example coverage

The coverage unit is one numbered item (`1. `, `2. `, ...) written at the
start of a line under a spec's `## Acceptance Examples` heading, up to the
next second-level heading, outside fenced code. Its id is
`RIPR-SPEC-NNNN#K`, where K is the item's own number.

A case may carry an optional `spec_examples` array of those ids: the
examples whose behavior its diff and runtime truth label. `validate`
rejects an id that is not `RIPR-SPEC-NNNN#K` (K positive, no leading zero),
a spec with no `docs/specs` file, a K that is not a numbered acceptance
example of that spec, a citation of an out-of-scope spec, and an id cited
twice by one case.

`fixtures/rust-verdict-corpus/spec-coverage.toml`
(`ripr_verdict_corpus_spec_coverage.v1`) is the ledger:

- `[[spec]]` names every spec with at least one numbered acceptance
  example, with `scope = "in"` or `scope = "out"`; an out-of-scope spec
  needs a one-line `reason`. In scope means the examples describe Rust
  analyzer verdict, oracle, probe or test-shape behavior that a corpus case
  (a Rust diff plus runtime mutant truth) can label. Output formats, CLI,
  editor, CI, agent, MCP, evaluation tooling, policy and non-Rust-language
  specs are out.
- `[[spec.waived]]` (`example`, `reason`) removes one example of an
  in-scope spec from the denominator when no corpus case can label it, for
  example a dead write where every mutant is equivalent
  (RIPR-SPEC-0228 examples 10 and 11).
- `[[unmeasured]]` (`id`, `reason`) lists in-scope specs whose acceptance
  examples are prose, not numbered, reported as `unmeasured_specs` rather
  than counted. The list is maintained by hand: nothing detects an in-scope
  prose spec that is missing from it.
- `floor` is the covered-example count the gate protects.

`validate` fails when a spec with numbered examples is missing from the
ledger (a new spec cannot leave the denominator silently), when the ledger
names a spec that does not exist, scopes a spec with no numbered examples,
lists as unmeasured a spec that now has numbered examples, waives an
example that does not exist or without a reason, or waives an example a
case covers, and when `floor` exceeds the coverable examples.

Covered examples are the in-scope, non-waived examples at least one case
cites. The report's `spec_example_coverage` section gives `coverage` as
covered over in-scope examples minus waived, `accounted` as covered plus
waived over in-scope examples, the in-scope, covered and waived example
counts, the in-scope and out-of-scope spec counts, `unmeasured_specs`, and
one row per in-scope spec with its in-scope examples, covered and waived
counts, and uncovered example numbers. Waived examples are never folded
into covered.

`check` fails before running ripr when covered is below `floor`, naming the
fall and the fix (restore the lost citation, or lower the floor with a
reason when a case was deliberately retired). When covered is above `floor`
it passes and prints that the floor can be raised. `report` and `validate`
print the coverage without gating it.

Decisions:

- The unit is the numbered acceptance example because it is the smallest
  thing a spec already promises and names. Prose acceptance examples have
  no stable identity, so they are listed, not counted.
- Coverage counts citations, not verdict agreement. A covered example says
  the corpus can measure that behavior; the verdict rates say whether ripr
  gets it right. Folding the two would let a wrong verdict raise coverage.
- The ledger names every spec with numbered examples, including
  out-of-scope ones, so a new spec forces a scoping decision in review
  instead of silently shrinking or growing the denominator. Rescoping a spec
  out or adding a waiver shrinks the denominator without failing `check`;
  only the ledger and golden-report diffs show it, so review owns that
  decision. When unsure, a spec is scoped in, where it shows as uncovered
  work.
- Waivers are per example with a reason and are reported apart from
  covered, so `accounted` can reach 1.0 while `coverage` stays honest.
- The gate protects a committed count, not a rate, so adding a newly
  scoped spec (which lowers the rate) passes while removing a citation
  fails.

## Required Evidence

- The committed corpus validates and contains both a `discriminated` and a
  `not_discriminated` case.
- Each validation rule rejects a tampered corpus.
- The scoring table, verdict projection, contradiction codes, and rate
  arithmetic are pinned by unit tests.
- The committed expected report agrees with the corpus labels row by row.
- The validator holds upstream subjects to a pinned URL, commit, and license
  file, and authored subjects to the `authored-` id prefix, no upstream
  provenance, no license file, and this repository's license; the report
  keeps authored rates apart from upstream rates. Relabeling a vendored
  excerpt as authored therefore means renaming the subject and every case
  that names it, which review sees; the validator cannot detect a rename
  that also strips the license file.

- Each spec-coverage validation rule rejects a tampered ledger or
  citation, the numbered-example reader handles continued and indented
  lines and specs without numbered items, and the floor gate fails below,
  passes at, and invites a raise above the floor.
- The committed ledger validates against `docs/specs` and its floor equals
  the covered count.

## Non-Goals

- Running mutation testing, `cargo test`, or network access from the
  harness. Truth was established once, at labeling, and is recorded.
- A population estimate. Rates describe these cases only.
- Replacing the judged panels or the shared Rust corpus; this corpus draws
  on the shared corpus pins where they exist.
- Wiring the check into CI; a regression gate consumes the report later.

## Acceptance Examples

- serde `format_u8` (`n >= 100` rewritten as `n > 99`): both boundary
  mutants fail `test_format_u8`, which checks all 256 values inside `loop`.
  ripr reports `reachable_unrevealed`, so the case scores
  `false_actionable`. A `static_limit` on the loop would score `abstained`.
- semver 1.0.23 `digit > b'9'` to `digit >= b'9'` (the first-run edit):
  four semver tests fail, so the tests discriminate the change. ripr 0.10's
  `reachable_unrevealed` would score `false_actionable`; the current
  `infection_unknown` scores `abstained`.
- itoa `remain > 9` rewritten as `remain >= 10`: the macro-generated tests
  still pass under both mutants, so a gap is ideal and ripr's
  `no_static_path` scores `abstained`.
- strsim `dacc84c` `src/lib.rs:195` `sim > 0.7`: the spot-check's `>=`
  miss is equivalent: no string pair up to 70 characters long gives a Jaro
  value of exactly 0.7. The case uses threshold shifts instead: 0.75 fails a
  test and 0.65 does not, so the truth is `partially_discriminated` and ripr's `weakly_exposed` scores
  `ideal`.
- authored `pricing-quote-total-field` (`subtotal + shipping` rewritten as
  `shipping + subtotal` in a struct literal): the only test asserting
  `total_cents` uses a free-shipping order, so dropping the shipping term
  passes while dropping the subtotal fails. The truth is
  `partially_discriminated`, and ripr's `exposed` scores `false_exposed`, in
  the authored rates only.
- authored `checkout-tax-self-computed-expected` (`subtotal * 8 / 100`
  rewritten as `subtotal * 2 * 4 / 100`): the only test's expected value is
  `sub + tax(sub)`, computed through the changed function, so every mutant
  passes and the truth is `not_discriminated`. ripr's `exposed` scores
  `false_exposed`, the self-computed expected value RIPR-SPEC-0004 and
  RIPR-SPEC-0035 say must not count as a strong oracle.
- bytesize `as_kb` division (`src/lib.rs:258`): ripr reports
  `no_static_path` while naming related tests, recorded as
  `no_static_path_with_related_tests`. semver `op()` at 1.0.23
  (`src/parse.rs:272`) carried `reach_yes_without_related_tests` until #5424
  named every examined test; its row now records no contradiction.

## Test Mapping

Tests live in `xtask/src/reports/verdict_corpus_tests.rs`:

- `score_follows_the_truth_table_in_both_error_directions`
- `finding_verdict_mirrors_triage_classes_and_named_limits`
- `anchored_findings_keep_only_candidate_current_findings_on_the_anchor`
- `declared_binding_reads_only_a_let_declaration`
- `anchored_findings_follow_a_retarget_only_for_the_anchor_binding`
- `case_verdict_ranks_gap_over_credit_over_limit`
- `contradictions_flag_each_internal_inconsistency_and_pass_a_clean_finding`
- `summary_contradictions_compare_counts_with_the_findings_list`
- `summary_contradictions_account_for_suppressed_findings`
- `ratio_text_is_fixed_precision_and_names_an_empty_denominator`
- `apply_patch_rewrites_the_anchored_line_and_reports_it_as_added`
- `apply_patch_refuses_drifted_context`
- `parse_patch_refuses_renames_and_empty_input`
- `parse_patch_holds_hunks_to_their_declared_counts_and_starts`
- `relativize_probe_files_strips_only_the_run_root`
- `committed_corpus_is_valid_and_measures_both_error_directions`
- `validator_rejects_a_label_that_contradicts_its_mutant_outcomes`
- `validator_rejects_an_expected_verdict_outside_the_truth_table`
- `validator_rejects_a_retained_file_whose_digest_moved`
- `validator_rejects_an_anchor_the_diff_does_not_add`
- `validator_rejects_ids_that_are_not_one_safe_path_segment`
- `validator_rejects_a_diff_that_patches_an_unretained_path`
- `validator_requires_both_truth_directions`
- `expected_report_rows_agree_with_corpus_labels`
- `build_report_counts_rates_over_the_right_denominators`
- `contradiction_counts_use_one_per_finding_unit`
- `stored_paths_keep_vendored_rust_out_of_the_workspace`
- `validator_holds_each_subject_origin_to_its_own_provenance`
- `report_keeps_authored_rates_apart_from_upstream_rates`

Spec-example coverage tests live in
`xtask/src/reports/verdict_corpus_coverage_tests.rs`:

- `numbered_examples_read_only_top_level_items_under_acceptance_examples`
- `spec_ids_come_from_the_spec_file_name`
- `example_ids_accept_only_the_canonical_spelling`
- `citations_of_malformed_unknown_or_unnumbered_examples_are_rejected`
- `citing_an_out_of_scope_spec_is_rejected`
- `an_example_both_waived_and_covered_is_rejected`
- `ledger_must_name_every_spec_with_numbered_examples_and_only_real_ones`
- `coverage_counts_cited_in_scope_examples_over_the_unwaived_ones`
- `floor_gate_fails_below_passes_at_and_invites_a_raise_above`
- `committed_ledger_is_valid_and_meets_its_floor`

## Implementation Mapping

- `xtask/src/reports/verdict_corpus.rs` owns validation, materialization,
  scoring, and rendering.
- `xtask/src/reports/verdict_corpus_coverage.rs` owns the numbered-example
  reader, the spec-coverage ledger and citation law, the coverage metric,
  and the floor gate.
- `fixtures/rust-verdict-corpus/spec-coverage.toml` is the ledger.
- `fixtures/rust-verdict-corpus/` holds the corpus, retained upstream and
  authored subjects, case diffs, and the expected report.

## Metrics

- `verdict_corpus_false_verdict_rate`
- `verdict_corpus_false_actionable_rate`
- `verdict_corpus_contradiction_rate`
- `verdict_corpus_spec_example_coverage` (dx-scoreboard
  `trust.verdict_corpus_spec_example_coverage`, target 1.0, regression
  margin 0)
