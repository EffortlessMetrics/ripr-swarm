# Fixture: module_reach_cfg_alt_reexport

Spec: RIPR-SPEC-0159

## Given

The library declares `inner` twice under complementary cfgs: `src/inner.rs` (inactive) and a `pub use std::cmp as inner;` re-export. The test imports `qual_reach::inner::order_discount`. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_cfg_alt_reexport
```

## Then

Not related: the library does not unconditionally bind `inner` to `src/inner.rs`.

Since #15877 the only `mod inner;` is behind `#[cfg(any())]`, so rustc never compiles `src/inner.rs`: the change seeds no finding and the outcome names the file as unreached by any Cargo target.

## Must Not

- Credit the test as calling the changed entry.
