# Fixture: rust_bare_name_return_tail_other_branch

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

The only test pins `discounted(100, true) == 90`, which takes the `member` branch and never returns the changed `amount`.

## When

```bash
cargo xtask fixtures rust_bare_name_return_tail_other_branch
```

## Then

The changed return reads `weakly_exposed`. The bare name reaches the return sink, but the only pin takes the `member` branch, so nothing observes the changed value.

```expected-classes
probe:src_lib.rs:return_value:69f8317e -> weakly_exposed
```

## Must Not

- Read the change as `exposed` because the test pins the owner's result exactly.
