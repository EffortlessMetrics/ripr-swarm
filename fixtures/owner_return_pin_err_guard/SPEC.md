# Fixture: owner_return_pin_err_guard

Spec: RIPR-SPEC-0197

## Given

The return of `weight(input)` changes from `input * 2` to `input * 3`.
The only test returns `Result` and checks the owner through a terminal
Err-return guard:

```rust
if weight(4) != 12 {
    return Err(format!("weight(4) was {}", weight(4)));
}
Ok(())
```

RIPR-SPEC-0154 reads the guard as its assertion twin
`assert!(weight(4) == 12)`: a lone top-level equality between the owner
call and an owner-free expected value.

## When

`cargo xtask fixtures owner_return_pin_err_guard` runs the public analyzer.

## Then

Exactly one return-value finding reads `exposed` through the owner-return
pin, the same credit `assert_eq!(weight(4), 12)` earns. This is the
positive control for the verdict-corpus case `checkout-fee-err-return-guard`.

## Must Not

- Read the guard below `exposed` while `assert_eq!(weight(4), 12)` pins.
- Credit an `==` guard, a range comparison, or an equality joined by
  `&&`/`||` (see `owner_return_pin_err_guard_not_equality`).
- Use mutation-runtime outcome vocabulary.
