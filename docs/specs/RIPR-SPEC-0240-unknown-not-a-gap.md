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

The refusal scope covers only related tests that could have credited an
oracle (`oracle_crediting_relations`, shared with reveal): a name-only
relation does not count while a reach-bearing test exists.

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

## Scope boundary

- Rust check findings only. The pilot seam rule is RIPR-SPEC-0230 (#5569).
- `weakly_exposed` findings are out of scope: a credited weak oracle is read
  evidence even when a stronger assertion was refused.
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
`static_unknown`. No fixture golden moves. The remaining false gaps are
`weakly_exposed` (sibling-arm oracles #5432; refused assertions behind a
credited weak oracle) and semver `op()` (reach with no retained test, #5344).

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

Tests in `crates/ripr/src/analysis/classify/owner_pin/tests.rs`:

- `analyzer_limit_refusals_are_limits_of_ripr_reading`
- `refusals_that_can_keep_an_assertion_from_running_are_not_limits`
- `an_outcome_settling_attribute_is_the_refusal_wherever_it_sits`

Integration: `crates/ripr/tests/owner_pin_execution.rs`
(`owner_pin_matched_static_and_runtime_controls`,
`macro_use_module_admission_matches_runtime`).

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
