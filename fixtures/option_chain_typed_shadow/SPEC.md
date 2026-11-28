# Fixture: option_chain_typed_shadow

Spec: RIPR-SPEC-0157

Related value transfer: RIPR-SPEC-0158. Honesty invariant: RIPR-SPEC-0108.

## Given

An immutable option-chain result is shadowed by a typed mutable binding with a tab after `let`. An alias overwrites the shadow with 2.

## When

The comparison changes from `n == 2` to `n < 3`. Both native assertions pass under each comparison.

## Then

Parsed binding patterns invalidate the original initializer across whitespace and type annotations. Both findings remain `infection_unknown`, without a computed `n == 0` observation.

## Must Not

- Credit the stale initializer as an exact discriminator.
- Promote these surviving assertions or omit either finding.
