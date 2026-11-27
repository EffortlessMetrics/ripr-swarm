# Fixture: rust_return_value_name_only_comparison_tail_far_input

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15274

## Given

The same edit as `rust_return_value_name_only_comparison_tail`: the tail
`n > LIMIT` becomes `n >= LIMIT`. The only test pins a value far from the
boundary: `assert_eq!(over_limit(5000), true)`. It passes on the old code
and on the new.

## When

`cargo xtask fixtures rust_return_value_name_only_comparison_tail_far_input` runs the public diff analyzer.

## Then

Neither finding reads `exposed`. The tail now reaches the return sink, but
no test input sits on the changed boundary, so both findings read
`weakly_exposed`, as they do for the literal twin `n >= 1000`.

## Must Not

- Promote either finding because the comparison is the returned tail. The
  tail sink establishes propagation only, never infection.
