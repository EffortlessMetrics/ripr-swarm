# Fixture: rust_predicate_tail_if_path_arms

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15274

## Given

`level` ends in a one-line `if` whose arms are the unit variants
`Level::High` and `Level::Low`, and the edit moves its condition from
`n > 10` to `n >= 10`. One test calls the owner at the boundary and pins
the variant: `assert_eq!(level(10), Level::High)`. It fails on the old code
and passes on the new.

## When

`cargo xtask fixtures rust_predicate_tail_if_path_arms` runs the public diff analyzer.

## Then

The `predicate` finding reads `exposed`. The condition decides the returned
value, so its sink is the return of `Level::High`, as it is for a tail `if`
whose arms are literals. It no longer reads `propagation_unknown`.

## Must Not

- Leave the finding at `propagation_unknown` because an arm is a path, not
  a literal.
