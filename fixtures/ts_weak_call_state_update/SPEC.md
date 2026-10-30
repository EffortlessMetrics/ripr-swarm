# Fixture: ts_weak_call_state_update

Spec: RIPR-SPEC-0243

## Given

The `Cart.add(amount)` edit of `ts_coincident_state_update`. The test first calls
`cart.add(5)` inside a weak `toBeDefined()` assertion, then pins `cart.add(0)` to `10`. The weak
call still moves the state, so the second assertion holds only under the new behavior.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_weak_call_state_update/input \
  --diff fixtures/ts_weak_call_state_update/diff.patch \
  --mode fast
```

## Then

The finding stays `exposed`: an owner call inside a weak assertion is not one the rule evaluates,
so the coincident-input rule does not apply.

## Must Not

- Count the weak assertion's call as accounted for and lower the finding (#10850).
