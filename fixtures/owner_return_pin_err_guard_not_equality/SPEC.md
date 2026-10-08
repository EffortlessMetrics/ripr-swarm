# Fixture: owner_return_pin_err_guard_not_equality

Spec: RIPR-SPEC-0197

## Given

The return of `weight(input)` changes from `input * 2` to `input * 3`.
Every related test compares the owner call, but none pins one value:

```rust
if weight(4) == 8 { return Err(..) }  // twin: assert!(weight(4) != 8)
assert!(weight(4) >= 12);
assert!(weight(4) == 12 || lenient);
```

## When

`cargo xtask fixtures owner_return_pin_err_guard_not_equality` runs the
public analyzer.

## Then

The return-value finding stays below `exposed`. The owner-return pin reads
operands only from a lone top-level `==` (an `assert!` condition or the
twin of a `!=` Err-return guard). An `==` guard's twin is an inequality,
`>=` admits many values, and `|| lenient` passes whatever the owner
returns.

**This fixture must NEVER read `exposed`.** It is the RIPR-SPEC-0108 false-credit control
for `owner_return_pin_err_guard`.

## Must Not

- Credit an `==` Err-return guard, a range comparison or an equality joined
  by `&&`/`||` as an owner-return pin.
- Use mutation-runtime outcome vocabulary.
