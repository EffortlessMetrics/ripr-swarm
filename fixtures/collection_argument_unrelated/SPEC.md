# Fixture: collection_argument_unrelated

Spec: RIPR-SPEC-0094

## Given

The push is rewritten from `x` to the neutral `x * 1`.

## When

The public check path reads the exact fixture and its literal assertion.

## Then

The collection effect findings read `reachable_unrevealed`: the test reaches
`add`, but `assert_eq!(11, 11)` compares only literals and reads no value, so
it supplies no oracle (#14968; before, they read `weakly_exposed`). The
runtime controls in
`owner_pin_execution::field_projected_collection` compile these sources and
run the named test against the neutral, wrong-value and deleted-push versions.

## Must Not

- Count compilation failure or zero executed tests as behavioral evidence.
- Borrow observation from another field, receiver or replaced binding.
