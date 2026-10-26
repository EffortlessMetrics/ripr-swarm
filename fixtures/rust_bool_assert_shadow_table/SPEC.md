# Fixture: rust_bool_assert_shadow_table

Spec: RIPR-SPEC-0353

## Given

`can_ship` is a pure bool `in_stock && paid` chain whose operands were reordered. The related test feeds the rows `(true, true, true)`, `(true, false, false)` and `(false, true, false)` through a constant `for` table. The test file defines a no-op `macro_rules! assert_eq` ahead of the tests, so the rows are called but never compared and a wrong bool implementation would pass every row (#10386).

## When

`ripr check` analyzes the reordered chain.

## Then

The finding is not `exposed` and its infection stays unknown: the truth-table rows are not credited when the assertion macro is rebound.

## Must Not

Promote the rows to an infection `yes` or supply a returned-value sink from a test whose `assert_eq!` compares nothing.
