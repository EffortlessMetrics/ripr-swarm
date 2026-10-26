# Fixture: module_reach_super_import_in_file_wrong_module

Spec: RIPR-SPEC-0159

## Given

`src/lib.rs` defines the crate-root entry `order_discount` over the changed private helper `is_bulk`, and an inline `pub mod flat` with its own `order_discount`. The in-file test module `flat::tests` does `use super::order_discount;`, where `super` is `flat`, not the crate root (#10279).

## When

```bash
cargo xtask fixtures module_reach_super_import_in_file_wrong_module
```

## Then

Not related: the relative import, resolved from its inline module, names `flat::order_discount`, not the entry that calls the changed helper.

## Must Not

- Credit the test as calling the changed entry.
