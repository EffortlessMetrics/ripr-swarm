# Fixture: module_reach_super_import_wrong_module

Spec: RIPR-SPEC-0159

## Given

The entry lives in `src/inner.rs`; the test does `use super::order_discount;`, where `super` is the crate root and not `inner`. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_super_import_wrong_module
```

## Then

Not related: the relative path does not resolve to the entry's module.

## Must Not

- Credit the test as calling the changed entry.
