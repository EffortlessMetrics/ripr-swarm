# Fixture: rust_predicate_appended_char_arm_old_input

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15029

## Given

The same edit as `rust_predicate_appended_char_arm`: `c == ','` becomes
`c == ',' || c == ';'`. The tests call the owner only with `','`, the
original arm's literal, and with `'.'`. Both tests pass on the old code and
on the new.

## When

`cargo xtask fixtures rust_predicate_appended_char_arm_old_input` runs the
public diff analyzer.

## Then

Neither finding reads `exposed`. The char tail now reaches the return sink,
as its integer twin does, but no test input equals the appended arm's
literal, so no input reaches the change.

## Must Not

- Promote either finding because the chain is the returned tail. The tail
  sink establishes propagation only, never infection.
