# Fixture: ts_coincident_state_update

Spec: RIPR-SPEC-0243

## Given

A TypeScript method `Cart.add(amount)` changes `this.total += amount` to
`this.total += amount * 2` and returns `this.total`. The only related test calls
`cart.add(0)` and pins the result to `0`. At `0` both versions add `0`, so the
assertion holds before and after the edit and notices nothing.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_coincident_state_update/input \
  --diff fixtures/ts_coincident_state_update/diff.patch \
  --mode fast
```

## Then

The finding stays below `exposed`: the call's literal input makes the old and new operand
agree, while a nearby input separates them, so the assertion is not a discriminator.

## Must Not

- Report `exposed` because a strong exact assertion reads the returned state (#10850).
