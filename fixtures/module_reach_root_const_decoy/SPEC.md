# Fixture: module_reach_root_const_decoy

Spec: RIPR-SPEC-0159

## Given

The library root declares `pub const order_discount` (an identity function) and a decoy module re-exporting `inner::order_discount`; there is no crate-root `pub use` of the entry. The test imports `qual_reach::order_discount` and calls the constant. The changed line is the private helper `is_bulk` inside `mod inner`.

## When

```bash
cargo xtask fixtures module_reach_root_const_decoy
```

## Then

Not related: a root value binding and a decoy-module re-export do not establish a crate-root re-export of the entry.

## Must Not

- Credit the test as calling `inner::order_discount`.
