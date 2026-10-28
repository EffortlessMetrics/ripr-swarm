# Fixture: rust_helper_expectation_literal_control

Spec: RIPR-SPEC-0197

## Given

`quota(2)` is asserted equal to `expected()`, a test-file helper that returns the independent literal `4` (#7714).

## When

`ripr check` analyzes the changed return value (`n + 1` -> `n + 2`).

## Then

The finding stays `exposed`: the helper does not depend on the owner.

## Must Not

Demote a helper-spelled expectation that never reaches the owner.
