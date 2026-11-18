# Fixture: rust_predicate_appended_int_arm

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15027

## Given

A `u32` predicate that is the owner's returned tail changes from `n == 1` to
`n == 1 || n == 2`. One test calls the owner with `2`, the appended arm's
literal, and asserts the result. Another calls it with `3` and asserts the
negation.

## When

`cargo xtask fixtures rust_predicate_appended_int_arm` runs the public diff
analyzer.

## Then

The predicate finding reads `exposed`. The input `2` makes the appended arm
true and the original arm false, so infection reads `yes`. Because the chain
is the returned tail, the change reaches the return, so propagation reads
`yes`. Before #15027 infection read `infection_unknown` and propagation read
unknown.

## Must Not

- Credit an input equal to an original arm's literal (see
  `rust_predicate_appended_int_arm_old_input`).
- Read spellings that may share one value (`05`, `5_u32`, `0x5`, `1_0`), a
  parameter that is not a primitive integer, or another parameter's arm.
