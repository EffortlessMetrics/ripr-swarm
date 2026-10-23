# Fixture: module_reach_namespace_alias_import

Spec: RIPR-SPEC-0159

Owner: analysis-fixtures

Issue: #8999

## Given

The test file aliases `use std as qual_reach;` before importing `qual_reach::inner::order_discount`. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_namespace_alias_import
```

## Then

Not related: the own-looking import prefix is rebound, so it does not name the library.

## Must Not

- Promote the finding on a counterexample, or lose the relation on the controls.
