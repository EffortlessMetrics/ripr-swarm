# Fixture: python_truthiness_swap_falsy_input_control

Spec: RIPR-SPEC-0028

## Given

The same `x is None` to `not x` change as
`python_truthiness_swap_no_exposed_credit`, but a test also calls `pick(0, 3)`
with a falsy non-None value and asserts the exact result.

## When

`cargo xtask fixtures python_truthiness_swap_falsy_input_control`

## Then

The finding stays `exposed`: the `0` call selects a different branch under the
two predicates, so the exact-value oracle notices the change.

## Must Not

Downgrade the finding when a strong related call places an input on which the
old and the new predicate disagree.
