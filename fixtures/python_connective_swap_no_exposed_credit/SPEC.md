# Fixture: python_connective_swap_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`ok` changes `return a > 0 and b > 0` to `return a > 0 or b > 0`. The only
test calls `ok(1, 2)` and `ok(-1, -2)`, where both comparisons are true or
both are false, so `and` and `or` give the same result, and asserts exact
values.

## When

`cargo xtask fixtures python_connective_swap_no_exposed_credit`

## Then

The finding is `weakly_exposed`, not `exposed`: no observed call makes the two
operands differ, so the suite passes with either connective. The missing
discriminator names a call where `a > 0` and `b > 0` differ. This is the
#16203 false-exposed correction.

## Must Not

Promote the finding to `exposed` while every strong related call gives both
operands the same truth value. The governed RIPR-SPEC-0108 honesty corpus
independently forbids promotion.
