# Fixture: module_reach_super_import_inline_module

Spec: RIPR-SPEC-0159

## Given

The relative import and the test sit inside `mod nested { .. }` in `src/unit.rs`, where `super` is the test module, not the crate root. The changed line is the private helper `is_bulk` in `src/lib.rs`.

## When

```bash
cargo xtask fixtures module_reach_super_import_inline_module
```

## Then

Not related: an inline module between the import and the file moves `super`.

## Must Not

- Credit the test as calling the changed entry.
