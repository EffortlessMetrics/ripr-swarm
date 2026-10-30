# Fixture: rust_call_chain_literal_input

Spec: RIPR-SPEC-0158

## Given

`is_adult` changed its boundary from `age > 18` to `age >= 18`. The related test calls the owner as `is_adult(age_from_env(18))` and `is_adult(age_from_env(30))`: each literal is the default passed to a helper that reads `AGE` from the environment, so the owner receives whatever the environment holds, not 18 or 30 (#10484).

## When

`ripr check` analyzes the changed boundary.

## Then

The finding is not `exposed`: the literals inside the helper call are not input rows, so the boundary value 18 is not shown to reach the owner.

## Must Not

Read a literal inside a call chain as the owner call's input, or credit the boundary from it.
