# Fixture: module_reach_local_module_self_import

Spec: RIPR-SPEC-0159

## Given

The test crate declares `mod qual_reach` and imports the entry through `self::qual_reach::inner::order_discount`. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_local_module_self_import
```

## Then

Not related: the import binds a module of the test crate.

## Must Not

- Promote the finding on a counterexample, or lose the relation on the controls.
