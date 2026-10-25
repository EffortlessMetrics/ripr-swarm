# Fixture: rust_wrapper_user_macro_guard

Spec: RIPR-SPEC-0186

Helper result forwarding follows RIPR-SPEC-0159.

## Given

A private boolean predicate changes from `qty >= 10` to `10 <= qty`. Its public
wrapper first invokes a crate-local `macro_rules! info` that returns `5` when
`qty == 10`, then forwards the predicate through an `if`. One integration test
pins the wrapper at 10 and 9.

## When

`ripr check` analyzes the predicate's changed line and its whole-return twin.

## Then

Neither finding is `exposed`. ripr does not expand macros, so a bare `info!`
is not trusted as the logging macro; only `tracing::`/`log::`-qualified
logging macros and the standard assertion/format macros stay transparent
(#10284, #10351).

## Must Not

Promote either finding from the wrapper's exact assertions. At `qty == 10` the
macro returns before the predicate runs, so the predicate mutations
`qty > 10`, `qty < 10` and `qty <= 10` leave `order_discount(10)` at `5`.
