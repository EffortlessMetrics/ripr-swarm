# Fixture: module_reach_path_remap_reexport

Spec: RIPR-SPEC-0159

## Given

The library remaps `inner` with `#[path = "foreign.rs"]`; `src/foreign.rs` is a std re-export, a binary root also declares `mod inner;` (so the file is compiled and a finding exists), while `src/inner.rs` holds the changed helper. The test imports `qual_reach::inner::order_discount`. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_path_remap_reexport
```

## Then

Not related: the library does not unconditionally bind `inner` to `src/inner.rs`.

## Must Not

- Credit the test as calling the changed entry.
