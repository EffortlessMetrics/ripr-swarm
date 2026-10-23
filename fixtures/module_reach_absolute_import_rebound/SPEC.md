# Fixture: module_reach_absolute_import_rebound

Spec: RIPR-SPEC-0159

Owner: analysis-fixtures

Issue: #8999

## Given

The test file aliases `use std as qual_reach;` and imports `::qual_reach::inner::order_discount` by an absolute path, then calls it bare. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_absolute_import_rebound
```

## Then

Not related: an absolute own-looking import goes through the same namespace proof as the bare prefix.

## Must Not

- Promote the finding on this counterexample.
