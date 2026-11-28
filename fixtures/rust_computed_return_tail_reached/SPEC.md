# Fixture: rust_computed_return_tail_reached

Spec: RIPR-SPEC-0443

## Given

Production code changes the computed express return from `base * 3` to
`base * 2`:

```rust
if express {
    base * 2
} else {
    base
}
```

The only test pins `shipping_cost(100, 10, true) == 800`. With those literal
arguments the earlier free-shipping guard folds false, `base` folds to `400`,
and `if express` takes the branch whose tail is the changed expression, so
the pinned `800` is the value of `base * 2`.

## When

```bash
cargo xtask fixtures rust_computed_return_tail_reached
```

or:

```bash
ripr check --root fixtures/rust_computed_return_tail_reached/input --diff fixtures/rust_computed_return_tail_reached/diff.patch --mode fast
```

## Then

The changed return reads `exposed` (#13037): the exact pin of a bare owner
call that statically reaches the changed branch confirms observation even
though no assertion text names `base * 2`.

```expected-classes
probe:src_lib.rs:return_value:f222f9b2 -> exposed
```

## Must Not

- Read the change as `weakly_exposed` only because the assertion does not
  repeat the expression's text.
