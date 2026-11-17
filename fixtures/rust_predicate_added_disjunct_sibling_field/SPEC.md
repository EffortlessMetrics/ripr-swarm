# Fixture: rust_predicate_added_disjunct_sibling_field

Spec: RIPR-SPEC-0158

Owner: analysis-fixtures

Issue: #14726

## Given

A struct-literal field initializer changes from `verbose: flag == "-v"` to
`verbose: flag == "-v" || flag == "--verbose"`. Since #14726 the predicate
reads the added `flag == "--verbose"` as its boundary. Both tests feed that
boundary input, but neither reads `verbose` from it. One asserts the sibling
field (`options_from_flag("--verbose").retries`). The other binds the
boundary call, asserts its `retries` and reads `verbose` only from a call on
the old input, `"-v"`, inside one tuple assertion.

## When

`cargo xtask fixtures rust_predicate_added_disjunct_sibling_field` runs the
public diff analyzer.

## Then

The predicate finding is not `exposed`: same-test boundary pairing for a
predicate on the changed field refuses an assertion that reads the owner's
record through any other field, even when another part of the same
assertion reads the changed field from a different call. Both tests pass on
the old and the new code.

## Must Not

- Pair the boundary input with an assertion that reads only `retries` from
  the boundary call.
- Let a changed-field read from a call on another input, placed beside a
  sibling-field read of the boundary binding in one tuple, stand in for a
  read of the boundary call's `verbose`.
