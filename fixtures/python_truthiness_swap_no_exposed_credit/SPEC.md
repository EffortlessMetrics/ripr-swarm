# Fixture: python_truthiness_swap_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`pick` changes its predicate from `x is None` to `not x`. The only test calls
`pick(None, 3)` and `pick(5, 3)`, the two inputs on which the old and the new
predicate select the same branch, and asserts exact values.

## When

`cargo xtask fixtures python_truthiness_swap_no_exposed_credit`

## Then

The finding is `weakly_exposed`, not `exposed`: no observed input selects a
different branch, so the suite passes with the old and the new predicate. The
missing discriminator names a falsy non-None input (`x == 0`). This is the
#9186 false-exposed correction.

## Must Not

Promote the finding to `exposed` while every strong related call gives the
parameter a value on which the two predicates agree. The governed
RIPR-SPEC-0108 honesty corpus independently forbids promotion.
