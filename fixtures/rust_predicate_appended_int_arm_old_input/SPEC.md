# Fixture: rust_predicate_appended_int_arm_old_input

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15027

## Given

The same edit as `rust_predicate_appended_int_arm`: `n == 1` becomes
`n == 1 || n == 2`. The tests call the owner only with `1`, the original
arm's literal, and with `3`. Both tests pass on the old code and on the new.

## When

`cargo xtask fixtures rust_predicate_appended_int_arm_old_input` runs the
public diff analyzer.

## Then

The predicate finding is not `exposed`. No test input equals the appended
arm's literal, so no input reaches the change.

## Must Not

- Promote the predicate because the chain is the returned tail. The tail sink
  establishes propagation only, never infection.
