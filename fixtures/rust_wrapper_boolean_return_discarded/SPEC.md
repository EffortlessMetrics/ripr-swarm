# Fixture: rust_wrapper_boolean_return_discarded

Spec: RIPR-SPEC-0186

Helper result forwarding follows RIPR-SPEC-0159.

## Given

A private boolean predicate changes from `qty >= 10` to `10 <= qty`. Its public
wrapper calls it but discards the result, then recomputes the discount. One test
pins the wrapper at 10 and 9.

## When

`ripr check` analyzes the predicate's changed line and its whole-return twin.

## Then

Both findings remain `propagation_unknown` with `helper_result_not_forwarded`.
The wrapper's exact assertions cannot confirm the discarded helper value.

## Must Not

Promote either finding from the wrapper's exact assertions. The predicate
mutations `qty > 10`, `qty < 10` and `qty <= 10` do not affect the wrapper's
recomputed result, so its tests cannot discriminate them.
