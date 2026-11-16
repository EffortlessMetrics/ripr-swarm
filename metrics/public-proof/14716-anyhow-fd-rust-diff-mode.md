# Mutation spot check

Agreement is scored only on canonical_precise records: a BinaryOperator or UnaryOperator mutant that ripr calibrate joined by seam_id or by unique span containment to a predicate_boundary or return_value seam, with a caught or missed outcome. File/line fallback joins, span-overlap ties, unmatched records, and unsupported genres, seam kinds or outcomes are counted as exclusions and never scored. Claims are limited to the recorded checkout revisions, cargo-mutants versions, and this join rule; this is not a suite adequacy measure.

## Repositories

| Repo | Revision | cargo-mutants | Mutants | Canonical precise | By seam ID | By span containment | Excluded |
| --- | --- | --- | --- | --- | --- | --- | --- |
| anyhow | `e621bd35dddd` | 27.1.0 | 176 | 12 | 0 | 12 | 164 |
| fd | `ce97e473ebae` | 27.1.0 | 503 | 136 | 0 | 136 | 367 |

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
| fd | `unscoreable_outcome` | 7 |
| fd | `unsupported_genre` | 125 |
| fd | `unsupported_seam_kind` | 29 |

## Scored verdicts (canonical precise joins)

| Verdict family | Seams | Mutants | Agree | Overclaim | False gap | Agreement |
| --- | --- | --- | --- | --- | --- | --- |
| claims_discriminator | 4 | 4 | 4 | 0 | 0 | 100.0% |
| claims_no_discriminator | 26 | 40 | 8 | 0 | 32 | 20.0% |

## Outcomes by disposition and grip class

| Disposition | Grip class | Outcomes |
| --- | --- | --- |
| ambiguous_span_overlap | no_joined_seam | caught 23, missed 9, unviable 6 |
| canonical_precise | activation_unknown | caught 78, missed 16 |
| canonical_precise | opaque | caught 5 |
| canonical_precise | strongly_gripped | caught 4 |
| canonical_precise | ungripped | caught 32, missed 8 |
| canonical_precise | weakly_gripped | caught 5 |
| file_line_only | activation_unknown | caught 8, missed 2, unviable 1 |
| file_line_only | opaque | unviable 3 |
| file_line_only | reachable_unrevealed | caught 3, missed 2, unviable 3 |
| file_line_only | strongly_gripped | caught 5, unviable 2 |
| file_line_only | ungripped | missed 1 |
| file_line_only | weakly_gripped | caught 4 |
| unmatched_no_containing_seam | no_joined_seam | caught 127, missed 38, unviable 32 |
| unscoreable_outcome | activation_unknown | unviable 6 |
| unscoreable_outcome | opaque | unviable 1 |
| unsupported_genre | activation_unknown | caught 66, missed 28, unviable 55 |
| unsupported_genre | opaque | caught 21, missed 7, unviable 17 |
| unsupported_genre | reachable_unrevealed | caught 2 |
| unsupported_genre | strongly_gripped | caught 5, unviable 3 |
| unsupported_genre | ungripped | caught 4, missed 2 |
| unsupported_genre | weakly_gripped | caught 7, missed 5, unviable 2 |
| unsupported_seam_kind | activation_unknown | caught 2, unviable 1 |
| unsupported_seam_kind | reachable_unrevealed | missed 3 |
| unsupported_seam_kind | strongly_gripped | caught 10, unviable 4 |
| unsupported_seam_kind | ungripped | caught 8, missed 2 |
| unsupported_seam_kind | weakly_gripped | caught 1 |

## Recall of missed mutants

123 runtime-missed mutants across all dispositions: activation_unknown 46, no_joined_seam 47, opaque 7, other 5, strongly_gripped 0, ungripped 13, weakly_gripped 5.

- Strict, flagged ungripped only: 13 (10.6%).
- Lenient, flagged or honest unknown (ungripped, weakly_gripped, activation_unknown, opaque): 71 (57.7%). Not flagged (strongly_gripped, no seam): 52.

Advisory; not suite adequacy.
- python: not established (no real-mutant corpus has been run for this language yet; no figure is claimed).
- rust: 123 missed; strict 13 (10.6%); flagged or unknown 71 (57.7%).
- rust_diff_mode: 123 missed; strict 19 (15.4%); flagged or unknown 65 (52.8%).
- typescript: not established (no real-mutant corpus has been run for this language yet; no figure is claimed).

## Seam coverage

679 mutants (420 caught, 123 missed; 543 with a caught or missed outcome). Canonical precise: 148 (21.8%). No containing seam: 197 (29.0%).

## Replaces cargo-mutants?

Verdict: `not_yet`. Not met: recall_flagged_or_unknown, scored_coverage. Not established: time_vs_full_cargo_mutants, python_typescript_real_mutant_agreement.

| Criterion | Bar and source | Observed | Status | Scope |
| --- | --- | --- | --- | --- |
| `precision_of_discriminator_claims` | 0.97 (docs/METRICS.md release ladder R2) | 1.0 | met | rust |
| `recall_flagged_or_unknown` | 0.8 (docs/METRICS.md release ladder R2) | 0.577 | not_met | rust |
| `recall_strict_ungripped` | n/a | 0.106 | informational | rust |
| `scored_coverage` | 0.5 (agent-chosen: a majority of generated mutants must reach a canonical_precise seam join and so receive a scored verdict) | 0.218 | not_met | rust |
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
- **false_gap** fd `src/filetypes.rs:29` predicate_boundary `!self.pipes && filesystem::is_pipe(*entry_type)` is `ungripped` (joined by span_containment); mutant `replace && with || in FileTypes::should_ignore` was caught.

## Operator-token diagnostic

Whether each canonical precise mutant's original operator appears in the joined seam's expression. Audit only: it never changes a disposition.

absent 1, present 147.
- **absent** anyhow `src/lib.rs:724` return_value `!*self` is `ungripped` (joined by span_containment); mutant `delete ! in __private::not::<impl Bool for &bool>::not` was caught.

## Rust diff-mode verdicts via ripr check --diff (real mutants, rust_diff_mode)

Advisory and never mixed with the Rust span-join figures, which score the same cargo-mutants mutants against repo-exposure seams. Here each mutant's own recorded cargo-mutants diff is applied to a scratch copy of the checkout and run through `ripr check --diff` (the product path), and the classification on the mutated lines is read back. Only pairs with a caught or missed outcome and exactly one classification and owner on the mutated lines are scored: exposed claims a discriminator (a missed mutant is an overclaim); reachable_unrevealed and no_static_path claim none (a caught mutant is a false gap); weakly_exposed and the unknown classes are counted but not scored. Every other mutant is an exclusion with its reason, and canonical_precise plus the exclusions equals records_total. A line with findings of different classes (a predicate and a return value, say) is excluded as ambiguous. Missed mutants that are excluded (ambiguous, no finding, a recorded diff that no longer matches, beyond the sampling cap) still count in recall_of_missed missed_total as not flagged, so the rust_diff_mode recall is a lower bound that depends on the cap. A mutated line is not a seam; the figures bind to the recorded revisions and cargo-mutants versions and are not suite adequacy.

679 mutants: 285 canonical_precise, 394 excluded.

| Exclusion | Mutants |
| --- | --- |
| `ambiguous_verdict` | 253 |
| `analysis_incomplete` | 3 |
| `no_finding_on_mutated_lines` | 2 |
| `unscoreable_outcome` | 136 |

| Verdict family | Owners | Mutants | Agree | Overclaim | False gap | Agreement |
| --- | --- | --- | --- | --- | --- | --- |
| claims_discriminator | 1 | 2 | 2 | 0 | 0 | 100.0% |
| claims_no_discriminator | 33 | 90 | 19 | 0 | 71 | 21.1% |
- **false_gap** anyhow `src/context.rs:37` diff_probe `self.context(context)` is `ungripped` (joined by synthetic_diff_verdict); mutant `FnValue [CaughtMutant] id=src/context.rs:37:13: replace ext::<impl StdError for Error>::ext_context -> Error with Default::default()` was caught.
- **false_gap** anyhow `src/context.rs:143` diff_probe `None` is `ungripped` (joined by synthetic_diff_verdict); mutant `FnValue [CaughtMutant] id=src/context.rs:143:9: replace <impl StdError for ContextError<C, E>>::source -> Option<&(dyn StdError +'static)> with None` was caught.
- **false_gap** anyhow `src/context.rs:148` diff_probe `nightly::provide(&self.error, request);` is `ungripped` (joined by synthetic_diff_verdict); mutant `FnValue [CaughtMutant] id=src/context.rs:148:9: replace <impl StdError for ContextError<C, E>>::provide with ()` was caught.
- **false_gap** anyhow `src/context.rs:157` diff_probe `None` is `ungripped` (joined by synthetic_diff_verdict); mutant `FnValue [CaughtMutant] id=src/context.rs:157:9: replace <impl StdError for ContextError<C, Error>>::source -> Option<&(dyn StdError +'static)> with None` was caught.
- **false_gap** anyhow `src/context.rs:157` diff_probe `Some(Box::leak(Box::new(Default::default())))` is `ungripped` (joined by synthetic_diff_verdict); mutant `FnValue [CaughtMutant] id=src/context.rs:157:9: replace <impl StdError for ContextError<C, Error>>::source -> Option<&(dyn StdError +'static)> with Some(Box::leak(Box::new(Default::default())))` was caught.
- **false_gap** anyhow `src/context.rs:162` diff_probe `Error::provide(&self.error, request);` is `ungripped` (joined by synthetic_diff_verdict); mutant `FnValue [CaughtMutant] id=src/context.rs:162:9: replace <impl StdError for ContextError<C, Error>>::provide with ()` was caught.
- **false_gap** anyhow `src/context.rs:182` diff_probe `Display::fmt(&s.escape_debug(), self.0)` is `ungripped` (joined by synthetic_diff_verdict); mutant `FnValue [CaughtMutant] id=src/context.rs:182:9: replace <impl Write for Quoted<&mut fmt::Formatter<'_>>>::write_str -> fmt::Result with Ok(Default::default())` was caught.
- **false_gap** anyhow `src/ensure.rs:76` diff_probe `self.written -= /* ~ changed by cargo-mutants ~ */ s.len();` is `ungripped` (joined by synthetic_diff_verdict); mutant `BinaryOperator [CaughtMutant] id=src/ensure.rs:76:22: replace += with -= in <impl Write for Buf>::write_str` was caught.
- **false_gap** anyhow `src/error.rs:658` diff_probe `unsafe { ErrorImpl::provide(self.inner.by_ref(), request) }` is `ungripped` (joined by synthetic_diff_verdict); mutant `FnValue [CaughtMutant] id=src/error.rs:658:9: replace Error::provide with ()` was caught.
- **false_gap** anyhow `src/error.rs:669` diff_probe `Self::provide(self, request);` is `ungripped` (joined by synthetic_diff_verdict); mutant `FnValue [CaughtMutant] id=src/error.rs:669:9: replace Error::thiserror_provide with ()` was caught.

## Pilot top recommendations

A pilot recommendation is confirmed when a viable operator mutant of its predicate or return expression was missed (with none, a viable mutant on its line; with none there either, a whole-body mutant of its innermost function), refuted when every such mutant was caught, and unscored otherwise. Precision is confirmed over confirmed plus refuted, for the recorded revisions and cargo-mutants versions only.

Top 10 per repository: 1 confirmed, 9 refuted, 10 unscored; precision 10.0%.

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
| fd | 9 | `src/dir_entry.rs:115` | return_value | weakly_gripped | refuted | line | 1 | 0 |
| fd | 10 | `src/dir_entry.rs:167` | return_value | weakly_gripped | confirmed | owner | 0 | 1 |
