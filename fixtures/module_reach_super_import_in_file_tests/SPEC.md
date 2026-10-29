# Fixture: module_reach_super_import_in_file_tests

Spec: RIPR-SPEC-0159

## Given

The default in-file unit-test layout (#10279): `#[cfg(test)] mod tests { use super::order_discount; .. }` in `src/lib.rs` beside the entry, calling it bare. The changed line is the private helper `is_bulk` in `src/lib.rs`.

## When

```bash
cargo xtask fixtures module_reach_super_import_in_file_tests
```

## Then

Related to the changed helper through the entry the relative import resolves to (control).

## Must Not

- Lose the relation on the control.
