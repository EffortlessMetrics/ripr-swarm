# Fixture: module_reach_super_import_nearer_shadow

Spec: RIPR-SPEC-0159

## Given

The same unit test, but its body declares a local `fn order_discount` that shadows the import. The changed line is the private helper `is_bulk` in `src/lib.rs`.

## When

```bash
cargo xtask fixtures module_reach_super_import_nearer_shadow
```

## Then

Not related: the bare call names the nearer local function.

## Must Not

- Credit the test as calling the changed entry.
