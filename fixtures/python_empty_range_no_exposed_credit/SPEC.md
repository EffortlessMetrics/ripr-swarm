# Fixture: python_empty_range_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`sum_to` changes `range(1, n + 1)` to `range(1, n)`. The only test asserts
`sum_to(0) == 0`; both ranges are empty at that input.

## When

`cargo xtask fixtures python_empty_range_no_exposed_credit`

## Then

The finding is not `exposed`: the sole exact-value assertion passes with both
range headers. This is the #12948 false-exposed correction.

## Must Not

Promote the finding to `exposed` while no observed input produces a different
iteration sequence. The RIPR-SPEC-0108 honesty corpus independently forbids
promotion.
