# Fixture: match_arm_tuple_terminal_inherited_constructor

Spec: RIPR-SPEC-0093

Corpus: RIPR-SPEC-0108 (`rust_terminal_inherited_constructor_no_promotion`)

Issue: [ripr#1728](https://github.com/EffortlessMetrics/ripr/issues/1728)

## Given

The full terminal proof owner is an out-of-line module using `use crate::*`.
Its request-only arm changes to `request_identity_v2`. A crate-root function
named `Some` shadows the prelude constructor and replaces every retained
relation with that literal. The test observes status, reason, request IDs and
receipt IDs. A native compiling wrong-arm mutant still passes those assertions.

## When

`cargo xtask fixtures match_arm_tuple_terminal_inherited_constructor`
analyzes the exact request-only diff.

## Then

The changed arm remains `weakly_exposed` with `observation_unverified`.
The indexed parser-owned value-name authority refuses the inherited constructor
instead of treating its name as the standard Option variant.

## Must Not

- Promote from unchanged body spelling or a constant returned relation.
- Borrow formatter authority as authority for a value constructor.
- Rewrite the consumer, suppress findings or relax thresholds.
