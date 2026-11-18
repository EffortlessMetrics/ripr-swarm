# Fixture: rust_predicate_appended_char_arm

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15027

## Given

A `char` predicate that is the owner's returned tail changes from `c == ','`
to `c == ',' || c == ';'`. One test calls the owner with `';'`, the appended
arm's literal. Another calls it with `'.'` and asserts the negation.

## When

`cargo xtask fixtures rust_predicate_appended_char_arm` runs the public diff
analyzer.

## Then

The predicate finding reads `exposed`: infection and propagation both read
`yes`, as for the integer arm.

## Must Not

- Read an escaped char (`'\x2c'`), an operator-like char (`'|'`, `'!'`), or a
  parameter that is not `char` (a `u8` compared with a char literal).
