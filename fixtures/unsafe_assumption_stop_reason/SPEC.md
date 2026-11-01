# Fixture: unsafe_assumption_stop_reason

Spec: RIPR-SPEC-0168

## Given

A crate adds the `unsafe` qualifier to `pub fn pick` (it becomes a caller
obligation) and adds an `unreachable_unchecked()` assumption in `first`
(#11549).

## When

`cargo xtask fixtures unsafe_assumption_stop_reason` runs the public analyzer.

## Then

Both changes stay `static_unknown` and name the unsafe obligation or the
unchecked assumption instead of "not mapped to a high-confidence change type".
Neither is `exposed`, and neither offers a test-stub repair step.

## Must Not

- Credit either change as `exposed` from a safe-code test.
- Offer an add-a-test repair for an unsafe caller obligation or assumption.
