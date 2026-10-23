# Fixture: module_reach_macro_expanded_shadow

Spec: RIPR-SPEC-0159

Owner: analysis-fixtures

Issue: #8999

## Given

The test crate exports a macro from `tests/common/mod.rs` that expands to `mod qual_reach { .. }`, invokes it in item position, and calls `qual_reach::inner::order_discount(..)`. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_macro_expanded_shadow
```

## Then

Not related: an item-position macro may declare a module named like the library.

## Must Not

- Promote the finding on this counterexample.
