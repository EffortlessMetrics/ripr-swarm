# Fixture: rust_computed_return_tail_wrapping_local

Spec: RIPR-SPEC-0443

## Given

The same change as `rust_computed_return_tail_reached` (`base * 3` to
`base * 2`), but the branch is chosen by `total > 250`, where
`let n: u8 = 200; let total = n + 100;`. With overflow checks off, `n + 100`
wraps to `44`, so `shipping_cost(100, 10, true)` takes the `else` branch and
returns `base`. The only test pins that value: `== 400`.

## When

```bash
cargo xtask fixtures rust_computed_return_tail_wrapping_local
```

or:

```bash
ripr check --root fixtures/rust_computed_return_tail_wrapping_local/input --diff fixtures/rust_computed_return_tail_wrapping_local/diff.patch --mode fast
```

## Then

The changed return is not promoted to `exposed` (#13037, #14950 review): a
fold that ignored the `u8` width would compute `300`, select the changed
branch, and credit a pin of the other branch's value.

```expected-classes
probe:src_lib.rs:return_value:f222f9b2 -> weakly_exposed
```

## Must Not

- Credit the reached-branch pin rule through arithmetic that wraps a
  narrower local.
