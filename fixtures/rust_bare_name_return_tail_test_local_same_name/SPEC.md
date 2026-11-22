# Fixture: rust_bare_name_return_tail_test_local_same_name

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

The only test binds `let amount = discounted(100, true);` and asserts `assert_eq!(amount, 90)`. The call takes the `member` branch.

## When

```bash
cargo xtask fixtures rust_bare_name_return_tail_test_local_same_name
```

## Then

The changed return reads `weakly_exposed`. The test binds the result to a local that happens to be named `amount`; that name is the test's own binding, not the owner's parameter, so it confirms nothing. The call takes the `member` branch.

```expected-classes
probe:src_lib.rs:return_value:69f8317e -> weakly_exposed
```

## Must Not

- Read the change as `exposed` because the assertion text names `amount`, the changed tail's spelling.
