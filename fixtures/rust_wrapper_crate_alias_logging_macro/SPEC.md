# Fixture: rust_wrapper_crate_alias_logging_macro

Spec: RIPR-SPEC-0186

Helper result forwarding follows RIPR-SPEC-0159.

## Given

A private boolean predicate changes from `qty >= 10` to `10 <= qty`. The crate
declares `use crate as tracing;` and an exported `macro_rules! info` that
returns `5` when `qty == 10`. The public wrapper calls `tracing::info!(qty)`,
which resolves to that user macro, then forwards the predicate through an
`if`. One integration test pins the wrapper at 10 and 9.

## When

`ripr check` analyzes the predicate's changed line and its whole-return twin.

## Then

Neither finding is `exposed`. ripr does not resolve macro path roots, so no
path-qualified macro is trusted to be transparent (#10284, #10351).

## Must Not

Promote either finding from the wrapper's exact assertions. At `qty == 10` the
macro returns before the predicate runs, so the predicate mutations
`qty > 10`, `qty < 10` and `qty <= 10` leave `order_discount(10)` at `5`.
