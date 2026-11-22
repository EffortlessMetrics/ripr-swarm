# Fixture: rust_bare_name_return_tail_reached

Spec: RIPR-SPEC-0443

Owner: analysis-fixtures

Issue: #13037

## Given

Production code changes the non-member branch of `discounted` from
`amount + 5` to the bare parameter `amount`:

```rust
if member {
    amount - 10
} else {
    amount
}
```

The only test pins `discounted(100, false) == 100`. With those literal arguments the owner takes the `else` branch, so the pinned `100` is the value of `amount`.

## When

```bash
cargo xtask fixtures rust_bare_name_return_tail_reached
```

## Then

The changed return reads `exposed`: the bare name reaches the return sink, and the exact pin of a bare owner call that statically takes the `else` branch observes its value (the RIPR-SPEC-0443 reached-tail pin).

```expected-classes
probe:src_lib.rs:return_value:69f8317e -> exposed
```

## Must Not

- Read the change as `propagation_unknown` because the returned tail spells no literal, call or arithmetic.
