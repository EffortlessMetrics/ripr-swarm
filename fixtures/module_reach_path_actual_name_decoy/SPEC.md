# Fixture: module_reach_path_actual_name_decoy

Spec: RIPR-SPEC-0159

Owner: analysis-fixtures

Issue: #8999

## Given

The library compiles `src/cmp.rs` as `actual` through `#[path = "cmp.rs"]` and declares an inline `pub mod cmp` re-exporting std. The test imports `qual_reach::cmp::order_discount`. The changed line is the private helper `is_bulk` in `src/cmp.rs`.

## When

```bash
cargo xtask fixtures module_reach_path_actual_name_decoy
```

## Then

Not related: the declaration chain for `cmp` does not reach `src/cmp.rs`.

## Must Not

- Credit the test as calling the changed entry.
