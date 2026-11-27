# Fixture: rust_return_value_name_only_comparison_tail

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15274

## Given

`over_limit` returns the comparison `n > LIMIT` as its whole tail, and the
edit changes it to `n >= LIMIT`. Neither operand is a literal, a call or
arithmetic. One test calls the owner at the boundary and pins the result:
`assert_eq!(over_limit(LIMIT), true)`. It fails on the old code and passes
on the new.

## When

`cargo xtask fixtures rust_return_value_name_only_comparison_tail` runs the public diff analyzer.

## Then

Both findings on the changed line read `exposed`. The `return_value`
finding reaches the return sink `n >= LIMIT`, the same sink its `predicate`
twin reads, and no longer reads `propagation_unknown` with an unknown sink.

## Must Not

- Leave the `return_value` finding at `propagation_unknown` because the
  returned comparison spells no literal.
