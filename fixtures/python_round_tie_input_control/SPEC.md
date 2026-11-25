# Fixture: python_round_tie_input_control

Spec: RIPR-SPEC-0028

## Given

`money` changes `return round(x, 0)` to `return round(x, 1)`. The only test
calls `money(1.05)`, a float spelling that sits on an exact decimal tie at the
finer count, and asserts the exact result of the new code.

## When

`cargo xtask fixtures python_round_tie_input_control`

## Then

The finding stays `exposed`: Python rounds `1.05` by the double it names, which
lies above the tie, so `round(1.05, 0)` is `1.0` and `round(1.05, 1)` is `1.1`,
and the exact-value assertion fails on the old code. Half-to-even on the
spelling alone gives `1.0` for both counts, so a float spelling on an exact
decimal tie is not read.

The expected static class per finding (machine-checked against
`expected/check.json` by `check-fixture-contracts`, #12928):

```expected-classes
probe:src_lib.py:python_preview:22f11de7 -> exposed
```

## Must Not

Downgrade the finding by reading a float spelling on a decimal tie as its
decimal digits.
