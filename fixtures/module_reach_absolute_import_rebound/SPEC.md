# Fixture: module_reach_absolute_import_rebound

Spec: RIPR-SPEC-0159

## Given

The test file aliases `use std as r#qual_reach;` (a raw-identifier rebinding of the library name) and then imports `qual_reach::inner::order_discount`, calling it bare. The absolute `::qual_reach::..` form resolves through the extern prelude, so it is covered as a positive control by `an_absolute_import_prefix_goes_through_the_same_proof`, not here. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_absolute_import_rebound
```

## Then

Not related: a raw-identifier alias of the library name rebinds the prefix.

## Must Not

- Promote the finding on this counterexample.
