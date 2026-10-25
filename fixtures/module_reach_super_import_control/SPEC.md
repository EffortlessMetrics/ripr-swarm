# Fixture: module_reach_super_import_control

Spec: RIPR-SPEC-0159

## Given

A `src/unit.rs` unit test declared `#[cfg(test)] mod unit;` in `src/lib.rs` that does `use super::order_discount;` and calls the crate-root entry bare. The changed line is the private helper `is_bulk` in `src/lib.rs`.

## When

```bash
cargo xtask fixtures module_reach_super_import_control
```

## Then

Related to the changed helper through the entry the relative import resolves to (control).

## Must Not

- Lose the relation on the control.
