# RIPR-SPEC-0319: Every named limitation and stop reason carries a next step

Status: proposed

Owner: product-analysis

Created: 2026-10-22

Linked issues: [#9438](https://github.com/EffortlessMetrics/ripr-swarm/issues/9438), [#10220](https://github.com/EffortlessMetrics/ripr-swarm/issues/10220).

Support-tier impact: None

The change is guidance text and its rendering only. Exposure classes, stage
states, evidence facts and support tiers are unchanged.

Policy impact: no dependency, network, workflow or security-policy changes.

## Problem

`StaticLimitKind::describe()` and `StopReason::describe()` say what ripr could
not resolve. Neither says what the reader can do about it, so a finding that
ends in a named limitation or stop reason leaves the reader with a diagnosis
and no direction. The classification already names the cause precisely; the
guidance can be keyed to it.

## Behavior

Every `StaticLimitKind` and every `StopReason` maps to one `next_step()`: a
single imperative sentence naming the concrete test shape, input or setting
that can let ripr settle the limitation. Where the limitation is an unresolved reach edge that may sit behind a private helper no test can name, the step is conditional ("if tests can name it") and says the edge can stay unresolved even with an exact assertion; it never promises that a test shape closes the edge. Both mappings are exhaustive `match`
expressions with no wildcard arm, so adding a variant does not compile until
it has guidance.

Human output renders the step as a `Next:` line directly under the
limitation or stop reason it belongs to (`ripr check --format human-full` and
`ripr explain`). Classification, JSON, SARIF and gate output are unchanged.

`ripr explain`'s "To change the verdict" recipe follows the same rule: when the finding retains an unresolved reach edge (transitive, public-API-path or macro reach, or the depth limit), the call-the-owner step reads "call X directly if tests can name it" and says the edge can stay unresolved even with an exact assertion, instead of "directly or through a helper" (#10220).

Next steps make no runtime, coverage or sufficiency claim: they say what to
add, not that the addition will pass or change the classification.

## Required Evidence

Unit tests walk every variant through an ordinal-checked `ALL` list and
require each step to be a single imperative sentence of bounded length, free of
runtime-outcome vocabulary, and distinct per limitation kind. Human-full
goldens re-bless with this spec as the reason; each drift is the added `Next:`
line only.

## Non-Goals

This spec does not cover refusals, analysis-limitation kinds, outcome kinds or
verdict classes (follow-up slices of the same guidance coverage work). It does
not derive per-finding values (a specific assertion or input); that remains
the job of the evidence-specific producers.

## Acceptance Examples

- A finding limited by `mocked_module` renders, under `Static limitation`,
  `Next: Add a test that runs the real implementation, with no mock replacing
  the changed module, and assert on its result.`
- A finding with stop reason `helper_identity_unresolved` renders a `Next:`
  line telling the reader to disambiguate the same-named functions.

## Test Mapping

- `domain::limit_guidance::tests::every_static_limit_names_an_actionable_next_step`
- `domain::limit_guidance::tests::every_stop_reason_names_an_actionable_next_step`
- `domain::limit_guidance::tests::unresolved_edge_next_steps_are_conditional_and_name_the_limit`
- `output::human::explain::tests::unresolved_reach_edge_recipe_does_not_promise_a_direct_call_closes_it`
- `domain::limit_guidance::tests::next_steps_are_distinct_per_kind`
- `domain::limit_guidance::tests::next_steps_make_no_runtime_or_coverage_claim`
- `domain::limit_guidance::tests::static_limit_kinds_all_is_a_gap_free_ordinal_sequence`
- `domain::limit_guidance::tests::stop_reasons_all_is_a_gap_free_ordinal_sequence`

## Implementation Mapping

- `crates/ripr/src/domain/limit_guidance.rs` owns the mappings.
- `crates/ripr/src/output/human/sections.rs` and
  `crates/ripr/src/output/human/explain.rs` render the `Next:` lines.

## Metrics

Count of next-step-bearing classification variants over total variants
(26 of 26 limitation kinds, 15 of 15 stop reasons).
