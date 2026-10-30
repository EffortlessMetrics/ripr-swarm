# Fixture: module_reach_body_local_use_shadow

Spec: RIPR-SPEC-0159

## Given

The test file imports `qual_reach::inner::order_discount` at file level, but the test body re-imports `foreign::order_discount`, which shadows it. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_body_local_use_shadow
```

## Then

Not related: a nearer function-local import binds the called name.

## Must Not

- Credit the test as calling the changed entry.
