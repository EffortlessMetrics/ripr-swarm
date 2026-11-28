# Fixture: rust_computed_return_tail_primitive_alias

Spec: RIPR-SPEC-0443

## Given

The same change as `rust_computed_return_tail_reached` (`base * 3` to
`base * 2`). The branch is chosen by `total > 250`, where
`let n: i32 = 200; let total = n + 100;`, but the file declares
`pub type i32 = u8;`, so `i32` names a one-byte type there. With overflow
checks off, `n + 100` wraps to `44`, and `shipping_cost(100, 10, true)` takes
the `else` branch. The test pins `== 800`, the value only the changed branch
would return.

## When

```bash
cargo xtask fixtures rust_computed_return_tail_primitive_alias
```

## Then

The changed return is not promoted to `exposed` (#13037, #14950 review): a
fold that trusted the primitive spelling would compute `300`, select the
changed branch, and credit the pin.

```expected-classes
probe:src_lib.rs:return_value:f222f9b2 -> weakly_exposed
```

## Must Not

- Credit the reached-branch pin rule in a file that rebinds a primitive type
  name.
