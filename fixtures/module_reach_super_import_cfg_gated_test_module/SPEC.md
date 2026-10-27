# Fixture: module_reach_super_import_cfg_gated_test_module

Spec: RIPR-SPEC-0159

## Given

The same unit test, but its module is declared `#[cfg(feature = "x")] mod unit;`, not a bare `#[cfg(test)]`. The changed line is the private helper `is_bulk` in `src/lib.rs`.

## When

```bash
cargo xtask fixtures module_reach_super_import_cfg_gated_test_module
```

## Then

Not related: the declaration chain to the test file is conditional.

## Must Not

- Credit the test as calling the changed entry.
