# Fixture: owner_return_pin_err_guard_shadowed_err

Spec: RIPR-SPEC-0197

## Given

The return of `weight(input)` changes from `input * 2` to `input * 3`.
The only test returns `Result` and checks the owner through a terminal
Err-return guard whose `Err` is shadowed by a value-namespace function
that returns `Ok(())`:

```rust
fn Err(_: &str) -> Result<(), String> {
    Ok(())
}

if weight(4) != 12 {
    return Err("mismatch");
}
```

The guard fires on the changed behavior, but its `Err` is not the
prelude variant: the test returns `Ok` and passes regardless. The guard
is not the twin of `assert!(weight(4) == 12)` in outcome, only in
spelling (identity over token coincidence, #7063 review).

## When

`cargo xtask fixtures owner_return_pin_err_guard_shadowed_err` runs the
public analyzer.

## Then

Exactly one return-value finding stays `weakly_exposed`: the guard
oracle exists, but the owner-return pin refuses it because the file
binds the value name `Err`, so the promotion
`checkout-fee-err-return-guard` earns cannot happen here. This is the
compiled wrong-implementation control for
`owner_return_pin_err_guard`.

## Must Not

- Promote to `exposed` while `owner_return_pin_err_guard` promotes: a
  shadowed `Err` must not earn the unshadowed guard's credit.
- Read the refusal as a gap the fixture cannot close by spelling the
  guard differently: only an unshadowed `Err` restores the twin.
- Use mutation-runtime outcome vocabulary.
