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

## Known Limitation

The golden also records a `return_value` finding at `exposed`. Inputs `1`
and `3` do not separate the old return from the new one, so that reading is a
false `exposed` on the return-value path. It predates #15027 and does not
come from the predicate path. Open PR #14194 (#8361) owns it. The honesty
corpus scopes this fixture's assertions to the predicate family for that
reason.
