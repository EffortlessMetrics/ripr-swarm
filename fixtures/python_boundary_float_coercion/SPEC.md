# Fixture: python_boundary_float_coercion

Spec: RIPR-SPEC-0028

## Given

`seconds(value)` opens with `value = float(value)` and changes `value < 0` to
`value <= 0`. The strong related tests call `seconds(1000)` and
`seconds(-10)`. Neither input is `0`, so pytest passes both before and after
the edit (#15287).

## When

`cargo xtask fixtures python_boundary_float_coercion`

## Then

The single finding is `weakly_exposed` and names `value == 0` as the missing
discriminator: the leading `float` coercion leaves each numeric literal equal,
so the call literals are the compared values and both miss the boundary.

## Must Not

Treat the coercion as an unreadable rebinding that keeps the oracle's
`exposed`. RIPR-SPEC-0108 enforces the class and `must_not_promote`
independently of the generated goldens.
