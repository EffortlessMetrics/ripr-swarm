# Fixture: rust_helper_expectation_self_computed

Spec: RIPR-SPEC-0197

## Given

`quota(2)` is asserted equal to `expected()`, a test-file helper that itself returns `quota(2)` (#7714). Both sides move with the owner, so no mutant of the owner can fail the test.

## When

`ripr check` analyzes the changed return value (`n + 1` -> `n + 2`).

## Then

The finding is not `exposed`: the expectation is computed by the owner through the helper, so no independent discriminator exists.

## Must Not

Credit the equality as a strong oracle through the helper's name.
