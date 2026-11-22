# Fixture: typescript_boundary_literal_in_other_parameter

Spec: RIPR-SPEC-0027

Additional contract: #15358.

## Given

The test passes the boundary literal `3_600_000` as the second parameter, `other`, which the owner never compares; the compared value `ms` is `5`. The owner's comparison changes from `>` to `>=`. The test asserts
`expect(formatShort(5, 3_600_000)).toBe("short")` with an exact-value matcher.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The predicate finding is `weakly_exposed`, never `exposed`: The literal sits in a parameter that does not feed the changed comparison.
The honesty corpus independently rejects promotion to `exposed`, even if the
golden report is regenerated.

## Must Not

- Treat a boundary literal that does not reach the changed comparison as a boundary witness.
- Infer a runtime mutation result from static evidence.
