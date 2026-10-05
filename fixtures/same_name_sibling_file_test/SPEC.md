# Fixture: same_name_sibling_file_test

Spec: RIPR-SPEC-0172

Owner: analysis-fixtures

Issue: #6537

## Given

Two sibling modules, `heaters` and `coolers`, each define `enum Mode` and
`fn delay(mode: Mode) -> u32` with the same match. Each file tests its own
`delay` from `mod tests { use super::*; .. }`: `heaters` asserts only
`Mode::Cold`, `coolers` asserts only `Mode::Warm`. The diff rewrites the
`Mode::Warm` arm of `heaters::delay` (`5` becomes `2 + 3`). Mutants `6`, `0`
and `1` of that arm all pass `cargo test` (rustc 1.95).

## When

`cargo xtask fixtures same_name_sibling_file_test` runs `ripr check` on the diff.

## Then

The arm reads `weakly_exposed`. Only `cold_delay` relates to
`heaters::delay` and supplies reach. `warm_delay` calls `delay` bare, and
`use super::*` in `coolers.rs` binds that name to `coolers::delay`, so the
test is not related to the changed function.

## Must Not

- Relate `warm_delay` to `heaters::delay`, or lend its `Mode::Warm` assertion
  as a confirming token for the changed arm.
- Read the arm `exposed` (the pre-fix result, confidence 1.00).
