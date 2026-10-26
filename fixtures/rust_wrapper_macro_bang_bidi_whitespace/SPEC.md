# Fixture: rust_wrapper_macro_bang_bidi_whitespace

Spec: RIPR-SPEC-0186

Helper result forwarding follows RIPR-SPEC-0159.

## Given

A private boolean predicate changes from `qty >= 10` to `10 <= qty`. The crate
defines `macro_rules! bail_at_ten`, which returns `5` when `qty == 10`. The
public wrapper invokes it as `bail_at_ten!` + U+200E + `(qty)`, then forwards
the predicate through an `if`. U+200E (left-to-right mark) is Rust lexer
whitespace, so rustc accepts the invocation. One integration test pins the
wrapper at 10 and 9.

## When

`ripr check` analyzes the predicate's changed line and its whole-return twin.

## Then

Neither finding is `exposed`. The invocation check skips Rust lexer whitespace
(Pattern_White_Space, including U+200E and U+200F) between `!` and the
delimiter, so the early-returning macro is still seen (#10284, #10351).

## Must Not

Promote either finding from the wrapper's exact assertions. At `qty == 10` the
macro returns before the predicate runs, so the predicate mutations
`qty > 10`, `qty < 10` and `qty <= 10` leave `order_discount(10)` at `5`.
