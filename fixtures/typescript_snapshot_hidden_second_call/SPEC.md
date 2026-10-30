# Fixture: typescript_snapshot_hidden_second_call

Spec: RIPR-SPEC-0258
Spec: RIPR-SPEC-0108

## Given

A TypeScript production module changes its return value from
`amount - 10` to `amount - 20`. The related test calls
`toMatchSnapshot()` once in the plain spelling and a second time through a
parenthesised reference (#8895):

```ts
expect(price(150)).toMatchSnapshot();
(expect(price(90)).toMatchSnapshot)();
```

A committed Jest `.snap` holds the entry `price 1` only. Counted as one call,
the entry would pin the value and read strong; the second call advances the
runner's counter, so the entry the first call uses is not the one named.

## When

```bash
cargo xtask fixtures typescript_snapshot_hidden_second_call
```

## Then

The snapshot oracle stays `snapshot` / medium and the finding stays
`weakly_exposed`: the parenthesised matcher name counts as a value reference
(two calls), so the committed entry is not credited.

## Must Not

- Credit the committed snapshot entry as a strong discriminator.
- Promote the finding to `exposed`.
