# Fixture: ts_dropped_term_zero_input

Spec: RIPR-SPEC-0248

## Given

A TypeScript owner `price(subtotal, fee)` drops the additive term `fee`
(`subtotal + fee + shipping(subtotal) + FLAT` becomes
`subtotal + shipping(subtotal) + FLAT`). The only related test asserts
`expect(price(6000, 0)).toBe(6499)` exactly. Its `fee` is `0`, so the test
passes on both versions.

The fixture workspace enables the TypeScript preview adapter via
`ripr.toml`:

```toml
[languages]
enabled = ["rust", "typescript"]
```

## When

```bash
ripr check \
  --root fixtures/ts_dropped_term_zero_input/input \
  --diff fixtures/ts_dropped_term_zero_input/diff.patch \
  --mode fast
```

## Then

The TypeScript preview adapter:

- reads the change as one dropped additive term `fee`,
- folds `fee` to `0` for the only tested call,
- keeps the finding `weakly_exposed` with discrimination `weak`, and names
  `fee` as 0 for every tested input.

## Must Not

- Promote the finding to `exposed` because the assertion is exact
  (the confirmed false promotion `ts-nodetest-quote-shipping-term`, #6654).
- Claim discrimination in the discriminate summary.
