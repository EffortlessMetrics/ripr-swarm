# Fixture: ts_seeded_state_update

Spec: RIPR-SPEC-0243

## Given

The `Cart.add(amount)` edit of `ts_coincident_state_update`. A `beforeEach` hook calls
`cart.add(5)`, so the state is `5` before the old code and `10` before the new code, and the
test's `cart.add(0)` pinned to `10` holds only under the new behavior. The test body shows only
the coincident `add(0)`.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_seeded_state_update/input \
  --diff fixtures/ts_seeded_state_update/diff.patch \
  --mode fast
```

## Then

The finding stays `exposed`: a call the test body does not show (the hook) seeded the state, so
the coincident-input rule does not apply and the existing verdict stands.

## Must Not

- Lower the finding because the visible call's input is a coincident `0` (#10850).
