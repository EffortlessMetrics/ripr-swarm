# RIPR-SPEC-0259: Next step follows the related oracle shape

Status: proposed

Owner: product-analysis

Created: 2026-10-13

Linked issues: [#7393](https://github.com/EffortlessMetrics/ripr-swarm/issues/7393).

Support-tier impact: None

The change is guidance wording only. Exposure classes, stage states, evidence
facts, oracle admission and support tiers are unchanged.

Policy impact: no dependency, network, workflow or security-policy changes.

## Problem

The generic `Next step` line attached to `weakly_exposed` findings is selected
from the probe family alone, so it presumes an assertion shape the related
test does not have. An `error_path` probe advises "assert the exact error
variant or payload instead of only is_err()" when the suite contains no
`is_err()` at all; a `return_value` probe advises "replace broad assertions"
when the only related test already is an exact `assert_eq!`. The sibling
`predicate` probe already names the real gap (the missing boundary input with
observed values), so one run tells the user to fix assertions they do not own
while a sibling finding holds the actionable direction. Following the wrong
advice literally finds nothing to change and erodes trust in every other
suggestion.

## Behavior

For a Rust `weakly_exposed` finding whose probe family is `error_path` or
`return_value`, the `recommended_next_step` text is selected from the
strongest related test's actual oracle kind instead of the probe family
alone:

- When the strongest related oracle (highest admitted strength rank) is
  already exact — `exact_value`, `exact_error_variant`,
  `whole_object_equality`, or `snapshot` — the family templates are not
  emitted. The step names the test. For `error_path`, it states that the gap
  is not the assertion shape and prescribes an input that executes the changed
  error arm with the exact variant or payload asserted. For `return_value`, an
  exact assertion may observe an effect rather than the returned value: first
  inspect whether the change can affect the return. Only if it can should the
  step prescribe a separating input with an exact returned-value assertion.
  Otherwise test the changed effect separately, without claiming that an input
  can distinguish identical returns. When the finding carries a missing
  discriminator fact, the step appends it the way the predicate probe's
  evidence does (`Missing discriminator: \`...\``).
- Only reach-establishing rows are eligible: the row's relation reason must
  establish reach to the changed owner
  (`RelationReason::establishes_reach`; `direct_owner_call`,
  `helper_owner_call`, `reexport_chain_followed`, or
  `assertion_target_affinity` on a non-function owner). A row that never
  establishes reach (`same_test_file`, `same_module`, `owner_named_test`,
  `weak_token_substring`, `import_path_affinity`,
  `fixture_owner_affinity`, `seam_callee_call`) never dictates the owner
  finding's advice, no matter how exact its oracle — that is the
  RIPR-SPEC-0094 token-coincidence control. A row whose annotated miss is
  `missing_exact_assertion` is also ineligible: there the assertion shape
  IS the gap. An `observation_unconfirmed` row stays eligible when it
  establishes reach: the suite's exact assertion exists and the step names
  the input, which is honester than presuming a broad assertion. Finally,
  the oracle must be admitted as `strong`: a syntactic `assert_eq!` the
  classifier downgraded (for example a self-computed expected value) does
  not establish an independent exact discriminator, so "keep the exact
  assertion" would be wrong advice for it and the family template stands.
  `build_finding` annotates each row's miss before selecting the step, so
  this eligibility reads the same final rows the JSON/human projections
  render.
- When the strongest related oracle is not exact (broad error check, smoke
  assertion, unreadable shape), or no related test row exists, the existing
  family template stands unchanged; that advice is honest for those shapes.
- Shapes with dedicated guidance are untouched: boxed wrapper-error seams
  keep the typed `wrapper_error_binding_unresolved` limitation step, and a
  changed `Err(...)` construction keeps the "reaches the changed error path"
  step (RIPR-SPEC-0106). Assertion-shaped owners keep the reframed oracle
  guidance (RIPR-SPEC-0133). `exposed` findings emit no next step.

## Required Evidence

Unit tests cover both families with an exact strong related oracle (the new
step names the test, prescribes the input, and appends the missing
discriminator), the return-value variant without a missing discriminator, and
the controls: a broad related oracle keeps the family template, a stronger
broad oracle outranking an exact one keeps it, empty related rows keep it,
and a non-`weakly_exposed` class emits nothing, plus the reach-based
eligibility controls: non-reach rows never dictate the step, an
`observation_unconfirmed` row that establishes reach stays eligible, and a
`missing_exact_assertion` row keeps the family template. Twenty-four fixture
goldens re-bless with `expected/CHANGELOG.md` reasons; each drift is confined
to the `recommended_next_step` string and its human rendering, with
classifications, stage states and evidence facts unchanged. The later
reach-based eligibility refinement re-blesses
`fixtures/owner_parameter_token_coincidence` under the same confinement: its
`same_test_file` token-coincidence row no longer dictates the step.

## Non-Goals

This spec does not change exposure classification, stage states, oracle
admission, reach/observation evidence, or the Python/TypeScript preview
adapters' own next-step producers. It does not attempt to derive the missing
input from observed values when the finding carries no missing-discriminator
fact, and it does not cover probe families outside `error_path` and
`return_value`.

## Acceptance Examples

- A `parse_port` change adding an `Ok(p) if p > 0` guard and a new
  `Err("port must be greater than zero")` arm, with the only test being
  `assert_eq!(parse_port("8080").unwrap(), 8080)`, no longer emits "replace
  broad assertions" or "instead of only is_err()"; both probes name the
  missing input and reference the exact assertion the test already has.
- A `read_all` change whose only test asserts `assert_eq!(got.ok(), Some(10))`
  no longer advises replacing a nonexistent `is_err()`; the step asks for an
  input that executes the changed error arm.
- A finding whose strongest related assertion is a bare `assert!(r.is_err())`
  keeps the "assert the exact error variant or payload instead of only
  is_err()" advice unchanged.

## Test Mapping

- `analysis::classify::decision::tests::exact_related_oracle_names_the_missing_input_for_error_path`
  covers the error-path step with a missing discriminator.
- `analysis::classify::decision::tests::exact_related_oracle_names_the_missing_input_for_return_value`
  covers the return-value step without one.
- `analysis::classify::decision::tests::non_exact_oracle_keeps_the_family_template`
  covers the broad-oracle, outranked, empty, and non-weak controls.
- `analysis::classify::decision::tests::non_reach_rows_do_not_dictate_the_step`
  covers the RIPR-SPEC-0094 token-coincidence control: `same_test_file`,
  `same_module`, `owner_named_test`, and `weak_token_substring` rows never
  dictate the step.
- `analysis::classify::decision::tests::unconfirmed_observation_on_a_reach_row_keeps_the_step`
  covers the reach-establishing `observation_unconfirmed` row staying
  eligible.
- `analysis::classify::decision::tests::missing_exact_assertion_row_keeps_the_family_template`
  covers the `missing_exact_assertion` exclusion.
- `fixtures/human_full_shared_parse_port` and the re-blessed golden set
  cover the rendered output end to end.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/decision.rs::exact_oracle_gap_next_step`
  owns the oracle-shape selection and step wording.
- `crates/ripr/src/analysis/classifier/finding.rs::build_finding` annotates
  each related row's miss before invoking the step selector, then invokes it
  only for `weakly_exposed` findings after the wrapper-seam and `Err(...)`
  construction special cases, passing the annotated related tests, the
  activation missing discriminators, and whether the owner is a function.

## Metrics

Report the count of `weakly_exposed` error-path and return-value findings
whose next step changed per run in the fixture re-bless. These are workload
measurements for the blessing record, not a growth bound.
