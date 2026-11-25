# Fixture: python_round_digits_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`money` changes `return round(x, 2)` to `return round(x, 3)`. The only test
calls `money(1.5)`, a number with one decimal place, where both versions return
`1.5`, and asserts the exact value.

## When

`cargo xtask fixtures python_round_digits_no_exposed_credit`

## Then

The finding is `weakly_exposed`, not `exposed`: no observed call passes a number
with more than two decimal places, so the suite passes with either digit count.
The missing discriminator names that input. This is a #16203 false-exposed
correction.

## Must Not

Promote the finding to `exposed` while every strong related call passes a
number both versions round alike. The governed RIPR-SPEC-0108 honesty corpus
independently forbids promotion.
