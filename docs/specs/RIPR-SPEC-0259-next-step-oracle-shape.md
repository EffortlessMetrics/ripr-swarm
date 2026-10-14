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
  emitted. The step names the test, states that the gap is not the assertion
  shape, and prescribes the missing input: for `error_path`, an input that
  executes the changed error arm with the exact variant or payload asserted;
  for `return_value`, an input where the changed branch behaves differently
  with the exact assertion kept. When the finding carries a missing
  discriminator fact, the step appends it the way the predicate probe's
  evidence does (`Missing discriminator: \`...\``).
- Only credited rows are eligible: the oracle must belong to a row whose
  oracle supports the finding's class, or whose only recorded gap is the
  input (`missing_input`). A row ripr could not credit (no call path, no
  observing assertion, an unconfirmed observation) never dictates the owner
  finding's advice, and the oracle must be admitted as `strong`: a syntactic
  `assert_eq!` the classifier downgraded (for example a self-computed
  expected value) does not establish an independent exact discriminator, so
  "keep the exact assertion" would be wrong advice for it and the family
  template stands.
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
and a non-`weakly_exposed` class emits nothing. Twenty-four fixture goldens
re-bless with `expected/CHANGELOG.md` reasons; each drift is confined to the
`recommended_next_step` string and its human rendering, with classifications,
stage states and evidence facts unchanged.

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
- `fixtures/human_full_shared_parse_port` and the re-blessed golden set
  cover the rendered output end to end.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/decision.rs::exact_oracle_gap_next_step`
  owns the oracle-shape selection and step wording.
- `crates/ripr/src/analysis/classifier/finding.rs::build_finding` invokes it
  only for `weakly_exposed` findings after the wrapper-seam and `Err(...)`
  construction special cases, passing the related tests and the activation
  missing discriminators.

## Metrics

Report the count of `weakly_exposed` error-path and return-value findings
whose next step changed per run in the fixture re-bless. These are workload
measurements for the blessing record, not a growth bound.
