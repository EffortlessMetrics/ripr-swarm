# Mutation spot check

Agreement is scored only on canonical_precise records: a BinaryOperator or UnaryOperator mutant that ripr calibrate joined by seam_id or by unique span containment to a predicate_boundary or return_value seam, with a caught or missed outcome. File/line fallback joins, span-overlap ties, unmatched records, and unsupported genres, seam kinds or outcomes are counted as exclusions and never scored. Claims are limited to the recorded checkout revisions, cargo-mutants versions, and this join rule; this is not a suite adequacy measure.

## Repositories

| Repo | Revision | cargo-mutants | Mutants | Canonical precise | By seam ID | By span containment | Excluded |
| --- | --- | --- | --- | --- | --- | --- | --- |
| anyhow | `e621bd35dddd` | 27.1.0 | 176 | 12 | 0 | 12 | 164 |
| fd | `ce97e473ebae` | 27.1.0 | 503 | 134 | 0 | 134 | 369 |

fd ran cargo-mutants with `--package=fd-find`.

## Exclusions

| Repo | Reason | Mutants |
| --- | --- | --- |
| anyhow | `ambiguous_span_overlap` | 2 |
| anyhow | `file_line_only` | 10 |
| anyhow | `unmatched_no_containing_seam` | 51 |
| anyhow | `unsupported_genre` | 99 |
| anyhow | `unsupported_seam_kind` | 2 |
| fd | `ambiguous_span_overlap` | 36 |
| fd | `file_line_only` | 24 |
| fd | `unmatched_no_containing_seam` | 146 |
| fd | `unscoreable_outcome` | 9 |
| fd | `unsupported_genre` | 125 |
| fd | `unsupported_seam_kind` | 29 |

## Scored verdicts (canonical precise joins)

| Verdict family | Seams | Mutants | Agree | Overclaim | False gap | Agreement |
| --- | --- | --- | --- | --- | --- | --- |
| claims_discriminator | 4 | 4 | 4 | 0 | 0 | 100.0% |
| claims_no_discriminator | 26 | 40 | 9 | 0 | 31 | 22.5% |

## Outcomes by disposition and grip class

| Disposition | Grip class | Outcomes |
| --- | --- | --- |
| ambiguous_span_overlap | no_joined_seam | caught 23, missed 8, unviable 7 |
| canonical_precise | activation_unknown | caught 70, missed 22 |
| canonical_precise | opaque | caught 3, missed 2 |
| canonical_precise | strongly_gripped | caught 4 |
| canonical_precise | ungripped | caught 31, missed 9 |
| canonical_precise | weakly_gripped | caught 5 |
| file_line_only | activation_unknown | caught 9, missed 1, unviable 1 |
| file_line_only | opaque | missed 1, unviable 2 |
| file_line_only | reachable_unrevealed | caught 3, missed 2, unviable 3 |
| file_line_only | strongly_gripped | caught 5, unviable 2 |
| file_line_only | ungripped | caught 1 |
| file_line_only | weakly_gripped | caught 4 |
| unmatched_no_containing_seam | no_joined_seam | caught 122, missed 44, timeout 1, unviable 30 |
| unscoreable_outcome | activation_unknown | timeout 2, unviable 6 |
| unscoreable_outcome | opaque | unviable 1 |
| unsupported_genre | activation_unknown | caught 61, missed 31, unviable 57 |
| unsupported_genre | opaque | caught 19, missed 8, unviable 18 |
| unsupported_genre | reachable_unrevealed | caught 2 |
| unsupported_genre | strongly_gripped | caught 6, missed 1, unviable 1 |
| unsupported_genre | ungripped | caught 5, missed 1 |
| unsupported_genre | weakly_gripped | caught 4, missed 8, unviable 2 |
| unsupported_seam_kind | activation_unknown | caught 2, unviable 1 |
| unsupported_seam_kind | reachable_unrevealed | missed 3 |
| unsupported_seam_kind | strongly_gripped | caught 9, unviable 5 |
| unsupported_seam_kind | ungripped | caught 10 |
| unsupported_seam_kind | weakly_gripped | caught 1 |

## Recall of missed mutants

141 runtime-missed mutants across all dispositions: activation_unknown 54, no_joined_seam 52, opaque 11, other 5, strongly_gripped 1, ungripped 10, weakly_gripped 8.

- Strict, flagged ungripped only: 10 (7.1%).
- Lenient, flagged or honest unknown (ungripped, weakly_gripped, activation_unknown, opaque): 83 (58.9%). Not flagged (strongly_gripped, no seam): 58.

Advisory; not suite adequacy.
- python: not established (no real-mutant corpus has been run for this language yet; no figure is claimed).
- rust: 141 missed; strict 10 (7.1%); flagged or unknown 83 (58.9%).
- typescript: not established (no real-mutant corpus has been run for this language yet; no figure is claimed).

## Seam coverage

679 mutants (399 caught, 141 missed; 540 with a caught or missed outcome). Canonical precise: 146 (21.5%). No containing seam: 197 (29.0%).

## Replaces cargo-mutants?

Verdict: `not_yet`. Not met: recall_flagged_or_unknown, scored_coverage. Not established: time_vs_full_cargo_mutants, python_typescript_real_mutant_agreement.

| Criterion | Bar and source | Observed | Status | Scope |
| --- | --- | --- | --- | --- |
| `precision_of_discriminator_claims` | 0.97 (docs/METRICS.md release ladder R2) | 1.0 | met | rust |
| `recall_flagged_or_unknown` | 0.8 (docs/METRICS.md release ladder R2) | 0.589 | not_met | rust |
| `recall_strict_ungripped` | n/a | 0.071 | informational | rust |
| `scored_coverage` | 0.5 (agent-chosen: a majority of generated mutants must reach a canonical_precise seam join and so receive a scored verdict) | 0.215 | not_met | rust |
| `time_vs_full_cargo_mutants` | 0.1 (agent-chosen: ripr must cost under a tenth of a full cargo-mutants run on the same crate to be a usable substitute in the edit loop) | ripr n/as, cargo-mutants n/as, ratio n/a | not_established | rust |
| `python_typescript_real_mutant_agreement` | n/a | n/a | not_established | python, typescript |

Bars: precision and recall from the R2 ladder rows in docs/METRICS.md, coverage and time ratio agent-chosen (see `threshold_source` in the JSON); a pass is not suite adequacy; scope is Rust on the recorded crates.

## Disagreement examples

- **false_gap** anyhow `src/lib.rs:717` return_value `!self` is `ungripped` (joined by span_containment); mutant `delete ! in __private::not::<impl Bool for bool>::not` was caught.
- **false_gap** anyhow `src/lib.rs:724` return_value `!*self` is `ungripped` (joined by span_containment); mutant `delete ! in __private::not::<impl Bool for &bool>::not` was caught.
- **false_gap** fd `src/cli.rs:701` predicate_boundary `!self.search_path.is_empty()` is `ungripped` (joined by span_containment); mutant `delete ! in Opts::search_paths` was caught.
- **false_gap** fd `src/filetypes.rs:23` predicate_boundary `!self.files && entry_type.is_file()` is `ungripped` (joined by span_containment); mutant `replace && with || in FileTypes::should_ignore` was caught.
- **false_gap** fd `src/filetypes.rs:23` predicate_boundary `(!self.files && entry_type.is_file()) || (!self.directories && entry_type.is_dir())` is `ungripped` (joined by span_containment); mutant `replace || with && in FileTypes::should_ignore` was caught.
- **false_gap** fd `src/filetypes.rs:24` predicate_boundary `!self.directories && entry_type.is_dir()` is `ungripped` (joined by span_containment); mutant `replace && with || in FileTypes::should_ignore` was caught.
- **false_gap** fd `src/filetypes.rs:25` predicate_boundary `!self.symlinks && entry_type.is_symlink()` is `ungripped` (joined by span_containment); mutant `replace && with || in FileTypes::should_ignore` was caught.
- **false_gap** fd `src/filetypes.rs:26` predicate_boundary `!self.block_devices && filesystem::is_block_device(*entry_type)` is `ungripped` (joined by span_containment); mutant `replace && with || in FileTypes::should_ignore` was caught.
- **false_gap** fd `src/filetypes.rs:27` predicate_boundary `!self.char_devices && filesystem::is_char_device(*entry_type)` is `ungripped` (joined by span_containment); mutant `replace && with || in FileTypes::should_ignore` was caught.
- **false_gap** fd `src/filetypes.rs:28` predicate_boundary `!self.sockets && filesystem::is_socket(*entry_type)` is `ungripped` (joined by span_containment); mutant `replace && with || in FileTypes::should_ignore` was caught.

## Operator-token diagnostic

Whether each canonical precise mutant's original operator appears in the joined seam's expression. Audit only: it never changes a disposition.

absent 1, present 145.
- **absent** anyhow `src/lib.rs:724` return_value `!*self` is `ungripped` (joined by span_containment); mutant `delete ! in __private::not::<impl Bool for &bool>::not` was caught.

## Pilot top recommendations

A pilot recommendation is confirmed when a viable operator mutant of its predicate or return expression was missed (with none, a viable mutant on its line; with none there either, a whole-body mutant of its innermost function), refuted when every such mutant was caught, and unscored otherwise. Precision is confirmed over confirmed plus refuted, for the recorded revisions and cargo-mutants versions only.

Top 10 per repository: 2 confirmed, 8 refuted, 10 unscored; precision 20.0%.

| Repo | Rank | Seam | Kind | Grip class | Verdict | Tier | Caught | Missed |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| anyhow | 1 | `src/chain.rs:30` | field_construction | weakly_gripped | unscored | none | 0 | 0 |
| anyhow | 2 | `src/chain.rs:68` | field_construction | weakly_gripped | refuted | owner | 1 | 0 |
| anyhow | 3 | `src/chain.rs:97` | field_construction | weakly_gripped | unscored | none | 0 | 0 |
| anyhow | 4 | `src/context.rs:54` | error_variant | weakly_gripped | unscored | none | 0 | 0 |
| anyhow | 5 | `src/context.rs:65` | error_variant | weakly_gripped | unscored | none | 0 | 0 |
| anyhow | 6 | `src/ensure.rs:43` | field_construction | weakly_gripped | unscored | none | 0 | 0 |
| anyhow | 7 | `src/error.rs:35` | error_variant | weakly_gripped | unscored | none | 0 | 0 |
| anyhow | 8 | `src/error.rs:163` | error_variant | weakly_gripped | unscored | none | 0 | 0 |
| anyhow | 9 | `src/error.rs:362` | field_construction | weakly_gripped | unscored | none | 0 | 0 |
| anyhow | 10 | `src/error.rs:478` | error_variant | weakly_gripped | refuted | owner | 1 | 0 |
| fd | 1 | `src/filter/size.rs:70` | predicate_boundary | weakly_gripped | refuted | seam | 1 | 0 |
| fd | 2 | `src/filter/size.rs:71` | predicate_boundary | weakly_gripped | refuted | seam | 1 | 0 |
| fd | 3 | `src/filter/size.rs:72` | predicate_boundary | weakly_gripped | refuted | seam | 1 | 0 |
| fd | 4 | `src/cli.rs:729` | predicate_boundary | weakly_gripped | refuted | seam | 1 | 0 |
| fd | 5 | `src/regex_helper.rs:8` | return_value | weakly_gripped | refuted | owner | 2 | 0 |
| fd | 6 | `src/regex_helper.rs:43` | return_value | weakly_gripped | refuted | owner | 2 | 0 |
| fd | 7 | `src/dir_entry.rs:31` | field_construction | weakly_gripped | unscored | none | 0 | 0 |
| fd | 8 | `src/dir_entry.rs:39` | field_construction | weakly_gripped | unscored | none | 0 | 0 |
| fd | 9 | `src/dir_entry.rs:115` | return_value | weakly_gripped | confirmed | line | 0 | 1 |
| fd | 10 | `src/dir_entry.rs:167` | return_value | weakly_gripped | confirmed | owner | 0 | 2 |
