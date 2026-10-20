# RIPR-SPEC-0240: Unknown, not a gap

Status: proposed

Owner: product-swarm

Created: 2026-10-04

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- release-0.11 trustworthy-verdicts scoreboard

Linked issues:

- #5327 (serde `format_u8` false gap), #5432 (sibling-arm oracles)

Linked PRs:

- #5318 (labeled Rust verdict corpus), #5359 (typed assertion refusals),
  #5416 (this spec)

Support-tier impact:

- None.
- This narrows when Rust findings claim a gap. It does not promote a
  language, surface, or gate.

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- Add one `static_limit_kind` value and one `stop_reason` value to
  `policy/output_contracts.txt`, `docs/OUTPUT_SCHEMA.md`, and
  `docs/STATIC_LIMITS.md`. The JSON schema version does not change; the
  values are additive.

## Problem

`reachable_unrevealed` tells a developer that the tests reaching a change do
not check it, and routes repair work. ripr reports it when every related
`assert_eq!` was refused, including refusals that only say ripr could not read
the evidence. On the labeled verdict corpus
([RIPR-SPEC-0219](RIPR-SPEC-0219-labeled-rust-verdict-corpus.md)) serde
`format_u8` is reported as a gap: `test_format_u8` checks all 256 values, but
it carries a feature `cfg` that ripr does not evaluate, so its `assert_eq!` was
refused.

A wrong actionable signal costs more than a missed advisory: the developer
digs, finds the test, and stops trusting the next finding.

## Behavior

A gap is a negative existential claim, so it needs a negative step that ripr
established from evidence it read. When the step rests on an analyzer limit,
the finding is an unknown that names the limit.

[RIPR-SPEC-0197](RIPR-SPEC-0197-owner-return-pins.md) records why each related
`assert_eq!` was refused (`AssertionRefusal`).
`AssertionRefusal::is_analyzer_limit` splits the refusals in two:

| Analyzer limit (gap withheld) | Shape that can keep the assertion from discriminating (gap kept) |
| --- | --- |
| lexical fallback, unparsed file, stale source | `if` branch or other conditional path |
| unplaced module, `include!`d file | closure that may not run or may exit early |
| unidentified test (also the fallback when no gate explains a refusal), `async fn` test | opaque macro or macro operand that may leave the test |
| any other test attribute: a feature `cfg`, `#[rstest]`, a lint attribute | `#[ignore]`, `#[should_panic]`, or a `cfg`/`cfg_attr` the shared evaluator (`attribute_test_build_availability`) reads as disabled in every test build |
| the same assertion spelled twice on one line | test nested in a function body; gated enclosing module or gated `mod` declaration |
| a binding that only may rebind the macro: foreign glob, unresolved `#[macro_use]`, macro argument, unparsed file, `no_implicit_prelude`, no site found | a `macro_rules!`/`macro` definition or `use` of the name in reach |

The right column is what the compiled runtime controls in
`crates/ripr/tests/owner_pin_execution.rs` prove are real gaps (the assertion
never runs, or runs as something else), plus attributes that decide the
outcome whatever the assertion does. The syntax scan names an
outcome-settling attribute as the refusal wherever it sits among the test's
attributes, ahead of `async` and other attributes, so a feature gate listed
first cannot hide an `#[ignore]`. A gated module stays a gap because
`cfg(any())` is indistinguishable from a feature gate at that refusal.
`cfg(false)` is read through the same shared evaluator, which does not yet
evaluate the literal, so a test gated only by it is withheld rather than kept.
Likewise `#[cfg_attr(test, ignore)]` and `#[cfg_attr(test, should_panic)]`
are not yet read as settling the outcome: the evaluator treats the introduced
attribute as ordinary, so the gap is withheld. Both cases err toward an
honest unknown, never toward a false gap.

An earlier `return` is a gap-keeping shape, with one value-guard exception
(trap-tally-reached-assert-control). The assertion is `assert_eq!(call, e)`
with `e` an integer literal. The test body's only `return` (outside closures
and nested items) is the whole body of a root statement
`if call <op> k { return; }` with no `else`, where `call` is the same text and one bare call `name(..)` (no path,
receiver or projection around it),
`k` is an integer literal and `<op>` is one comparison. If that condition
does not hold at `e` and also spares at least one wrong value, the
assertion runs for the value it expects and for some value it rejects, and
which one a mutant returns is a value ripr does not evaluate. A `!=` guard
spares no wrong value: the assertion runs only when it passes, so it stays a
certain gap. ripr does not know the integer type, so a neighbour of `e`
counts only when the guard's own threshold holds it (`> k` with `e < k`
spares `e + 1`) or every type holding `e` holds it: `e - 1` for `e >= 1`
or a negative `e` above every signed minimum, `e + 1` for an `e` that is no
type's maximum. `if count() > 0 { return; }` before
`assert_eq!(count(), 0)` therefore stays a gap. An `==` guard spares every
wrong value but `k`, including a default-like `k` such as `0` (#7932 tally
review). `AssertionRefusal::ValueGuard` is then an analyzer limit, and the
classifier counts it only when `call` is the owner's call. When the
condition holds at `e`, this exception does not apply and the existing
guarded-return refusal stands: its twin `trap-steps-early-return`
(`if steps(2) < 1000 { return; }` before `assert_eq!(steps(2), 14)`, which
also returns early for every wrong value below 1000) stays
`reachable_unrevealed`. A guard that holds at `e` but at no wrong value
(`== e`) lets every wrong value reach the assertion; it still reads as a
gap and is a separate, pre-existing case (#7044). A call with a
string or char literal argument stays out, since matching the guard's
call ignores whitespace. Both cases' labeled mutants were run (runtime mutant
kill truth in the verdict corpus). An unconditional `return;` before the
assertion, which the runtime controls pin, is untouched.

The refusal scope covers only related tests that could have credited an
oracle (`oracle_crediting_relations`, shared with reveal): a name-only
relation does not count while a reach-bearing test exists.

For a string-valued predicate, a shared reason literal in an unrelated
same-file/module assertion cannot supply observation or discrimination while
an actual owner-reaching test exists (#7763). Keep that assertion visible with
`no_call_path`. A possible opaque or transitive owner call retains the existing
allowance. This does not resolve formatted/local input activation or turn a
runtime validation bypass rejection into static exposure.

The classifier marks a `reachable_unrevealed` finding whose observe summary is
`rust_assertion_context_unestablished` when it has at least one refused related
`assert_eq!` and every one is an analyzer limit.
`analysis::classify::gap_admission::withhold_unsupported_gap` is the single
owner of the decision. It runs last in the Rust limit post-pass
(`apply_probe_and_oracle_limits`, diff and repo mode), after every
producer-named limit, consumes the marker, and applies only when the finding is
still `reachable_unrevealed` with `observe` `no` and no `static_limit_kind`.

A matching finding becomes:

- `classification`: `static_unknown`;
- `static_limit_kind`: `rust_assertion_context_unresolved`;
- `stop_reasons`: `gap_evidence_unresolved`, once;
- `confidence`: rescored for `static_unknown`, which drops the bonus a gap
  earns; a lower score from an earlier limit pass is kept;
- `missing`: the limit's one-sentence description, replacing the absence
  lines that stated the withheld claim;
- `activation.missing_discriminators`: empty, so no repair placement, repair
  card, or agent packet is built from a withheld gap;
- `evidence`: a `gap_withheld: reachable_unrevealed is not claimed; ...` line
  plus the RIPR-SPEC-0114 limitation detail lines (last established edge
  naming up to three related tests, first unresolved edge, analyzer route
  `analysis/classify/gap-admission`, non-claim). The RIPR-SPEC-0197
  `assertion not credited:` line stays and says why;
- `recommended_next_step`: names the related tests and says ripr does not
  report a gap.

The stage evidence is unchanged, so `evidence_path` still shows what ripr
read. The rule never upgrades a class.

### Panic outcome of a `#[should_panic]` test

A `#[should_panic]` test with no assertion still has an oracle: it fails
when the code it runs does not panic. ripr reads such a test as having no
assertion, so a changed panic guard (`if index >= 100 { panic!(..) }`)
reached only by that test read as a certain gap even when every mutant that
stops the panic fails it. `gap_admission::withhold_should_panic_gap` withholds
that gap under `rust_panic_outcome_unresolved` when all of these hold:

- the finding is `reachable_unrevealed` with no `static_limit_kind`;
- the changed line is inside the owner function, and the owner's body
  (comments and strings masked) invokes a panicking macro (`name!(`,
  `name![` or `name!{`) (`panic!`,
  `unreachable!`, `todo!`, `unimplemented!`, `assert!`, `assert_eq!`,
  `assert_ne!` or a `debug_assert` form) on the changed line or after it;
- a related test with a `direct_owner_call` relation has no assertion and
  carries `#[should_panic]` without `#[ignore]`, either bare, with
  `expected = ""` (which matches any message), or with `expected = ".."`
  whose text is in a string literal argument of a panicking macro invoked
  on or after the changed line. A literal elsewhere in the owner does not
  count, and neither does a `format!` template. Other attribute forms
  (`#[should_panic = ".."]`, a raw string, `cfg_attr`) keep the gap.

It runs in `apply_probe_and_oracle_limits` after the log-capture limit and
before `withhold_unsupported_gap`, and writes the finding the same way, with
`next_step` naming the tests. The panic is not credited: a bare
`#[should_panic]` also passes on a panic from another callee, so its twin
`trap-row-of-unrelated-panic` (`Grid::new(0)` panics first) moves from gap
to limited, which its case accepts. An `expected` message the owner never
raises on or after the changed line, a change after the owner's last panic,
an owner with no panicking macro, a test without `#[should_panic]`, and a
`#[should_panic]` test with an assertion, and an ignored test keep the gap; only the matching
test's start line binds it to its related row.
`spec0227-check-variant-should-panic` is untouched: it reads as
`weakly_exposed` (the test's `unwrap`), and its owner has no panicking macro.
A panicking macro inside a nested `fn`, closure or `macro_rules!` in the
owner still counts; that over-reads toward `static_unknown`, never toward a
gap or credit.

### Call-fact budget of a related test line (#8600)

`extract_call_facts` keeps at most 256 distinct callees per source line. A
related test with a line at that budget may have had a call dropped, and the
dropped call may be the test's assertion helper or a second owner call, so a
negative claim over that test's calls is not established. The shared owner
`withhold_call_budget_gap` reads the existing saturation predicate
(`has_saturated_line`); no facts or cache field carries a flag. A
`reachable_unrevealed` or `weakly_exposed` finding with a saturated related
test becomes `static_unknown` under `rust_call_fact_budget` and drops its
absence claims. An `infection_unknown`, `propagation_unknown` or
`static_unknown` finding keeps its class and gains the reason. `exposed`,
`no_static_path`, a finding with a producer-named limit, and a finding whose
related tests have no saturated line are untouched. Exactly 256 is
conservatively limited: it does not prove a call was dropped. Saturation is read over every matched related test before the eight-row render cap trims the rows, and a saturated test's `no_call_path`, `no_assertion`, `missing_input` and `assertion_not_observing` row misses read as `observation_unconfirmed` (`weak_assertion` and `missing_exact_assertion` describe an assertion ripr read and stay). An already-unknown finding drops its call-derived absence lines (no oracle, no discriminator, no exact variant, no boundary input, missing-discriminator values), clears its missing discriminators and replaces its next step, as a withheld gap does. The reason never
raises a class. Scope: the finding surfaces (`check`, JSON, human, SARIF, LSP).

The `repo-exposure-json` seam reading is a separate producer
(`test_grip_evidence`), so it applies the same decision to its stage
evidence through `test_grip_evidence::call_budget`, over the same shared
predicate and the same typed `rust_call_fact_budget` kind. Saturation is read
over every ranked related test before the compact twelve-test cap. A stage
that read `no` or `weak` (activation, propagation, observation,
discrimination) becomes `unknown` and names the reason, and the seam's
missing boundary values are dropped, so a `reachable_unrevealed` or
`weakly_gripped` seam reads as an unknown class (`discrimination_unknown`,
`propagation_unknown`, ...) and its evidence record is `static_limitation`
with a `limitation` canonical item, no repair command, and a
`static_limitations[]` row whose `category` is `rust_call_fact_budget`. An
already-unknown stage keeps its class and gains the reason in its summary.
A strongly gripped seam (every stage `yes`, no missing value), an `ungripped`
seam and a seam whose reach is not `yes` or `weak` are untouched. The compact
reading behind `repo-exposure-summary-json` applies the same function, and the
classified-seam cache generation moved to `1.85` so a warm cache cannot serve
the pre-limit actionable record.

Decision (#8600): an independent empty-slice assertion beside a saturated
slice table stays `infection_unknown`, and ripr does not credit it. The
unsaturated twin with the same independent assertion is also
`infection_unknown`, because a literal-slice call is outside the eager
slice-table family that resolves the boundary; the call-fact budget adds only
the typed reason. Crediting the assertion would be a new table-family rule
that also changes unsaturated inputs, and it would move a finding toward
`exposed` (a clean claim), not toward a repair signal, so it needs its own
corpus and decision. `call_budget_slice_positive.rs` pins the three readings.
### Framework oracles ripr does not evaluate

Four test idioms check a changed value through machinery ripr reads only at
the surface, and each used to leave a certain `weakly_exposed` claim.
`framework_oracle::withhold_framework_oracle_gap` withholds a
`weakly_exposed` or `reachable_unrevealed` finding with no
`static_limit_kind` under `rust_framework_oracle_unresolved` when a
non-ignored related test matches one of these shapes:

- **environment**: a `match_arm`, `predicate` or `error_path` probe, a
  `direct_owner_call` test with a strong assertion that calls
  `set_var("NAME", ..)` or `remove_var("NAME")`, and an owner body that calls
  `env::var("NAME")` or `env::var_os("NAME")` with the same literal name,
  with the owner called after the first such write. A different variable, a
  non-literal name, a write after the call or a relational-only test keeps the
  gap (`lib-serial-test-batch-default-relational`).
- **mock closure**: a test with a strong assertion and an `expect_<m>()` call
  whose `.returning(|..|)` closure uses one of its named (non-`_`) parameters,
  where the changed line calls `.<m>(`. A closure that ignores its arguments,
  or a changed line that does not call the mocked method (the grace predicate
  of `lib-mockall-grace-boundary-times`), keeps the gap.
- **expect-test**: a `direct_owner_call` test comparing a non-empty
  `expect![[..]]` literal with `.assert_debug_eq(..)`, where the asserted
  operand is a call to the owner or a local whose live binding where the
  assertion runs was made by `let [mut] x = ..owner(..)`. A later `let`
  (even on the same line, in a closure or callback body the assertion sits in,
  or behind an attribute), an assignment or `&mut x` borrow, a binding closed
  with its block, and a binding made only after the assertion do not reach it.
- **insta**: a `direct_owner_call` test with a snapshot macro that has no
  inline `@` literal, whose snapshotted value (after an optional literal name)
  derives from the owner as above (a live binding at the assertion), and no `add_redaction`, `with_settings` or `Settings`,
  on a changed `field:` initializer that no redaction selector (a literal
  keyed by `=>`, such as `".id"` or `"[].id"`) names. Any other changed line
  keeps the gap (the `snapshot_oracle` fixture's changed predicate), and so
  does a redacted field (`lib-insta-json-redacted-id`).

Any one matching related test withholds the finding: a weaker sibling test
reads as evidence ripr gathered, not as the oracle that decides, and the
matching test is the strongest oracle ripr cannot evaluate. The shapes read
the test text, as code only: a write, binding or assertion that appears only
in a comment, a nested comment or a string (raw strings included) is not
there, while a real literal name or expectation is read from the original at
the same offsets. A macro that does not qualify is skipped, so a later one that
does still counts. Two limits are accepted: the mocked-method match reads the
method name, not the receiver, and only related tests (the first eight per
finding) are judged; either can withhold a gap, which only costs an advisory.

It runs after the should-panic rule and before `withhold_unsupported_gap`,
and writes the finding the same way. Nothing is credited. Corpus effect:
`lib-serial-test-retries-default-arm`, `lib-mockall-recipient-predicate`,
`lib-expect-test-assert-debug-eq-variant-payload` and
`lib-insta-json-unredacted-sibling-field` move from `false_actionable` to
`abstained`, which their cases accept.

## Scope boundary

- Rust check findings only. The pilot seam rule is RIPR-SPEC-0230 (#5569).
- `weakly_exposed` findings are out of scope except for the framework
  oracles above: a credited weak oracle is read evidence even when a stronger
  assertion was refused.
- Two broader rules were built and dropped. Withholding every refused-context
  gap hid the runtime-controlled gaps above. Withholding `weakly_exposed` when
  a strong oracle's text does not name the changed arm hid the sibling-arm
  gaps that the `match_arm_*` integration tests pin (#5432).
- A third, withholding reach with no retained related test, moved no corpus
  case and withheld four correct fixture gaps, so it was dropped too.

## Required Evidence

- Unit tests pin the withholding, its confidence rescore, idempotence, the
  marker contract, and that named limits and non-gap classes are untouched.
- Unit tests pin `is_analyzer_limit` for each refusal kind and attribute.
- A paired fixture: a default-feature `cfg` test is withheld
  (`gap_withheld_feature_gated_test`); an `#[ignore]` test keeps the gap
  (`gap_kept_ignored_test`).
- Every runtime control in `owner_pin_execution` and `match_arm_*` keeps its
  class. The one control that changes, an unresolved `#[macro_use]` whose test
  catches the mutant at runtime, moves from a false gap to `static_unknown`.
- On the verdict corpus, false actionable falls and false silent and false
  exposed do not rise.

## Measured effect

Verdict corpus (203 cases, corpus 2026-10-04.8), measured against main at
80975c807 (#5359 merged):

| Rate | Before | After |
| --- | --- | --- |
| False actionable (of discriminated) | 58/106 | 57/106 |
| False actionable, upstream cases | 7/20 | 6/20 |
| False exposed (of not fully discriminated) | 6/97 | 6/97 |
| False silent (of not fully discriminated) | 0/97 | 0/97 |
| Abstained | 58/203 | 59/203 |

The first measurement, on the 34-case corpus 2026-10-04.3 on top of #5359,
moved false actionable on upstream cases from 9/20 to 8/20 through the same
single case.

serde `format_u8` hundreds moves from `reachable_unrevealed` to
`static_unknown`. No existing fixture golden changes class. The remaining false
gaps are `weakly_exposed` (sibling-arm oracles #5432; refused assertions behind
a credited weak oracle) and semver `op()` (reach with no retained test, #5344).

## Non-Goals

- Crediting any assertion, or changing equality admission.
- Evaluating Cargo features or `cfg` predicates.
- A population claim. The rates describe the corpus only.

## Acceptance Examples

- serde `format_u8`, `n >= 100` rewritten as `n > 99`: `test_format_u8`
  carries `#[cfg(any(feature = "std", not(no_core_net)))]`, ripr refuses its
  `assert_eq!`, and the finding is `static_unknown` with
  `rust_assertion_context_unresolved`, naming `test_format_u8`.
- `owner_return_pin_if_false`: the assertion sits in `if false { .. }`; the
  finding stays `reachable_unrevealed`.

## Test Mapping

Tests in `crates/ripr/src/analysis/classify/gap_admission.rs`:

- `analyzer_limit_refusals_withhold_the_gap_and_name_the_test`
- `withholding_drops_the_gap_confidence_bonus_and_never_raises_a_score`
- `an_unmarked_refusal_keeps_the_gap`
- `a_marker_without_the_refused_observe_keeps_the_gap_and_is_consumed`
- `an_existing_named_limit_and_non_gap_classes_are_untouched`
- `withholding_twice_adds_one_stop_reason`
- `a_panic_macro_is_read_by_its_own_name`
- `a_should_panic_test_expects_the_owner_panic_only_when_the_message_matches`

Tests in `crates/ripr/src/analysis/classify/framework_oracle.rs` and
`crates/ripr/tests/framework_oracle_limit.rs`:

- `a_mock_closure_counts_only_when_it_reads_a_named_argument`
- `an_arm_chosen_by_the_environment_is_withheld` and its keeps-the-gap control
- `an_expect_debug_literal_is_withheld` and its keeps-the-gap control
- `an_unredacted_insta_field_is_withheld` and its keeps-the-gap control

Tests in `crates/ripr/src/analysis/classify/owner_pin/tests.rs`:

- `analyzer_limit_refusals_are_limits_of_ripr_reading`
- `refusals_that_can_keep_an_assertion_from_running_are_not_limits`
- `an_outcome_settling_attribute_is_the_refusal_wherever_it_sits`
- `a_return_guard_that_spares_the_expected_value_is_an_analyzer_limit`
- `a_direct_call_name_is_one_bare_call_ending_the_subject`

Integration: `crates/ripr/tests/owner_pin_execution.rs`
(`owner_pin_matched_static_and_runtime_controls`,
`macro_use_module_admission_matches_runtime`) and
`crates/ripr/tests/should_panic_limit.rs`
(`a_should_panic_test_on_a_changed_panic_guard_reads_as_static_unknown`,
`a_panic_outcome_the_rule_does_not_cover_keeps_the_gap`) and
`crates/ripr/tests/call_budget_limit.rs`
(`a_saturated_related_test_line_withholds_the_gap_under_the_budget_reason`,
`saturation_that_does_not_limit_the_proof_keeps_the_finding`,
`an_independently_established_positive_keeps_its_class`,
`a_saturated_test_past_the_related_row_cap_still_withholds_the_gap`,
`an_already_unknown_finding_names_the_budget_and_drops_call_derived_absence`), plus the unit test
`gap_admission.rs::tests::a_related_test_line_at_the_call_budget_withholds_a_gap_and_names_an_unknown`.

Fixtures: `gap_withheld_feature_gated_test`, `gap_kept_ignored_test`.

Wire strings are pinned by
`crates/ripr/src/domain/language.rs::tests::static_limit_kind_wire_strings_are_stable`
and `crates/ripr/src/domain/probe.rs::tests::stop_reason_glosses_are_clean_prose`.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/owner_pin.rs`:
  `AssertionRefusal::is_analyzer_limit`.
- `crates/ripr/src/analysis/classifier/evidence.rs` computes the refusal
  scope; `crates/ripr/src/analysis/classifier/finding.rs` marks the finding.
- `crates/ripr/src/analysis/classify/gap_admission.rs` owns the decision.
- `crates/ripr/src/analysis/language/rust/mod.rs` calls it last in
  `apply_probe_and_oracle_limits`.
- `crates/ripr/src/domain/language.rs` and `crates/ripr/src/domain/probe.rs`
  own the new wire values.

## Metrics

- `verdict_corpus_false_actionable_rate`
- `verdict_corpus_false_verdict_rate`
