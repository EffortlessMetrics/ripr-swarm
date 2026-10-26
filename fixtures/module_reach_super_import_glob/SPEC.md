# Fixture: module_reach_super_import_glob

Spec: RIPR-SPEC-0159

## Given

The same unit test with a `use std::collections::*;` glob beside the relative import. The changed line is the private helper `is_bulk` in `src/lib.rs`.

## When

```bash
cargo xtask fixtures module_reach_super_import_glob
```

## Then

Not related: a glob in the test file may bind or unbind the entry's name.

## Must Not

- Credit the test as calling the changed entry.
