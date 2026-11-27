# Fixture: python_sort_direction_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`top` changes `return sorted(xs)[0]` to `return sorted(xs, reverse=True)[0]`.
The only test calls `top([4])`, a one-element list that sorts the same both
ways, and asserts the exact value.

## When

`cargo xtask fixtures python_sort_direction_no_exposed_credit`

## Then

The finding is `weakly_exposed`, not `exposed`: no observed call passes a list
holding two different elements, so the suite passes with either direction. The
missing discriminator names that input. This is a #16203 false-exposed
correction.

## Must Not

Promote the finding to `exposed` while every strong related call passes a list
whose elements all compare equal. The governed RIPR-SPEC-0108 honesty corpus
independently forbids promotion.
