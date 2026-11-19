# Fixture: rust_guarded_boundary_guard_missing

Spec: RIPR-SPEC-0001

## Given

Production code moves a guarded boundary from `>` to `>=`:

```rust
if order_cents > FREE_OVER_CENTS && !express {
```

to:

```rust
if order_cents >= FREE_OVER_CENTS && !express {
```

The only test pins `shipping_cost(100, 5000, true) == 200`. The call sits on
the boundary value `5000`, but with `express = true` the guard is false under
both versions, so the boundary never decides the result and the call cannot
tell `>` from `>=`.

## When

```bash
cargo xtask fixtures rust_guarded_boundary_guard_missing
```

or:

```bash
ripr check --root fixtures/rust_guarded_boundary_guard_missing/input --diff fixtures/rust_guarded_boundary_guard_missing/diff.patch --mode fast
```

## Then

The changed predicate reads `infection_unknown`, and the reason names the
comparison `order_cents >= FREE_OVER_CENTS`, the guard `express = false` and
the boundary value (#13037).

## Must Not

- Credit the call `shipping_cost(100, 5000, true)` as boundary evidence: it
  misses the guard (the compound-predicate reading of #9637 read the
  comparison without its guard and promoted it to `exposed`).
- Report the guarded boundary as an actionable missing discriminator.
