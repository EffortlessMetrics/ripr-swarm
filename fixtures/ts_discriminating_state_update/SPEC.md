# Fixture: ts_discriminating_state_update

Spec: RIPR-SPEC-0243

## Given

The same `Cart.add(amount)` edit as `ts_coincident_state_update`, with a test that calls
`cart.add(5)` and pins the result to `10`. At `5` the old code returns `5` and the new code
returns `10`, so the assertion fails under the old behavior.

The fixture workspace enables the TypeScript preview adapter via `ripr.toml`.

## When

```bash
ripr check \
  --root fixtures/ts_discriminating_state_update/input \
  --diff fixtures/ts_discriminating_state_update/diff.patch \
  --mode fast
```

## Then

The finding stays `exposed`: the literal input separates the old and new operand. This is
the positive control for #10850; the coincident-input fix must not lower it.

## Must Not

- Lower the finding because the call's input is a literal: `5` separates `amount` from `amount * 2`.
