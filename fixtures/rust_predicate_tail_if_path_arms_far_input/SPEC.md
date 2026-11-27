# Fixture: rust_predicate_tail_if_path_arms_far_input

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15274

## Given

The same edit as `rust_predicate_tail_if_path_arms`: the tail `if` over
`Level::High` and `Level::Low` moves its condition from `n > 10` to
`n >= 10`. The only test pins a value far from the boundary:
`assert_eq!(level(50), Level::High)`. It passes on the old code and on the
new.

## When

`cargo xtask fixtures rust_predicate_tail_if_path_arms_far_input` runs the public diff analyzer.

## Then

The `predicate` finding does not read `exposed`. The condition now reaches
the return sink, but no test input sits on the changed boundary, so it
reads `weakly_exposed`.

## Must Not

- Promote the finding because the arms are now read as returned values.
  The sink establishes propagation only, never infection.
