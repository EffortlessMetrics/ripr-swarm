# Fixture: python_round_coarse_count_control

Spec: RIPR-SPEC-0028

## Given

The same `round` digit-count change as `python_round_digits_no_exposed_credit`,
but with negative counts: `money` changes `return round(x, -2)` to
`return round(x, -3)`. The only test calls `money(60)`, whose leading digit
sits below the coarser count's place, and asserts the exact result of the new
code.

## When

`cargo xtask fixtures python_round_coarse_count_control`

## Then

The finding stays `exposed`: `round(60, -2)` is `100` and `round(60, -3)` is
`0`, so the exact-value assertion fails on the old code. A count whose place
lies past a value's leading digit rounds the value to zero; reading the two
counts alike there would wrongly downgrade the finding.

The expected static class per finding (machine-checked against
`expected/check.json` by `check-fixture-contracts`, #12928):

```expected-classes
probe:src_lib.py:python_preview:e7f16398 -> exposed
```

## Must Not

Downgrade the finding by reading `round(60, -2)` and `round(60, -3)` as alike.
