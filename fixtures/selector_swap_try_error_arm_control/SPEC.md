# Fixture: selector_swap_try_error_arm_control

Spec: RIPR-SPEC-0107

## Given

The same scheduler shape as `selector_swap_try_not_error_path`, but the edit
changes the propagated error variant (`QueueError::Empty` to
`QueueError::Closed`) on a line ending in `?`. This is a real changed `?` error
arm.

## When

```bash
cargo xtask fixtures selector_swap_try_error_arm_control
```

## Then

The line stays an `error_path` finding; the selector-swap abstention of #9866
does not apply because no selector name changed.

The expected static class per finding (machine-checked against
`expected/check.json` by `check-fixture-contracts`, #12928):

```expected-classes
probe:src_lib.rs:error_path:d4885999 -> weakly_exposed
```

## Must Not

- Let the #9866 abstention hide a real changed error arm.
