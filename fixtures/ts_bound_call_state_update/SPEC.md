# Fixture: ts_bound_call_state_update

Spec: RIPR-SPEC-0243

## Given

The `Cart.add(amount)` edit of `ts_coincident_state_update`. The test binds `cart.add` to a local,
calls `invoke(5)`, then pins `cart.add(0)` to `10`. The bound call still moves the state, so the
second assertion holds only under the new behavior.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_bound_call_state_update/input \
  --diff fixtures/ts_bound_call_state_update/diff.patch \
  --mode fast
```

## Then

The finding stays `exposed`: a mention of the owner that is not a direct call (a bound, extracted
or destructured method value) can move the state unseen, so the coincident-input rule does not apply.

## Must Not

- Count only `name(` mentions and lower the finding while a bound call moves the state (#10850).
