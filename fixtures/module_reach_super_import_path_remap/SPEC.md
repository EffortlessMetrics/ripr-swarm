# Fixture: module_reach_super_import_path_remap

Spec: RIPR-SPEC-0159

## Given

The same unit test, but `src/lib.rs` also carries a `#[path = "other.rs"] mod other;` remap. The changed line is the private helper `is_bulk` in `src/lib.rs`.

## When

```bash
cargo xtask fixtures module_reach_super_import_path_remap
```

## Then

Not related: a `#[path]` remap anywhere in the package can move the module layout the path is read from.

## Must Not

- Credit the test as calling the changed entry.
