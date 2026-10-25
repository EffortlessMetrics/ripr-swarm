# Fixture: rust_wrapper_unicode_macro_suffix

Spec: RIPR-SPEC-0186

Helper result forwarding follows RIPR-SPEC-0159.

## Given

A private boolean predicate changes from `qty >= 10` to `10 <= qty`. The crate
defines `macro_rules! éprintln`, which returns `5` when `qty == 10`. Its name
ends in `println` but binds no standard macro name. The public wrapper calls
`éprintln!(qty)`, then forwards the predicate through an `if`. One integration
test pins the wrapper at 10 and 9.

## When

`ripr check` analyzes the predicate's changed line and its whole-return twin.

## Then

Neither finding is `exposed`. The macro path is read as one identifier token
including its non-ASCII characters, so `éprintln` is not mistaken for the
standard `println` (#10284, #10351).

## Must Not

Promote either finding from the wrapper's exact assertions. At `qty == 10` the
macro returns before the predicate runs, so the predicate mutations
`qty > 10`, `qty < 10` and `qty <= 10` leave `order_discount(10)` at `5`.
