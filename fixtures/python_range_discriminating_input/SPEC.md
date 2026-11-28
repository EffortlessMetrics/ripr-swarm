# Fixture: python_range_discriminating_input

Spec: RIPR-SPEC-0028

## Given

`sum_to` changes `range(1, n + 1)` to `range(1, n)`. The test asserts
`sum_to(3) == 6`; the old range yields six and the new range yields three.

## When

`cargo xtask fixtures python_range_discriminating_input`

## Then

The finding is `exposed`: the exact-value assertion distinguishes the changed
iteration sequence. This is the positive control for #12948.

## Must Not

Withhold exposure when an observed input produces different returned sums.
