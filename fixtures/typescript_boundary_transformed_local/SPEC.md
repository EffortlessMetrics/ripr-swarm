# Fixture: typescript_boundary_transformed_local

Spec: RIPR-SPEC-0027

Additional contract: #15358.

## Given

The test passes the boundary literal `3_600_000` as `ms`, but the compared local is `Math.abs(ms) / 2`, which is `1_800_000`, not the boundary. The owner's comparison changes from `>` to `>=`. The test asserts
`expect(formatShort(3_600_000)).toBe("short")` with an exact-value matcher.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The predicate finding is `weakly_exposed`, never `exposed`: The tested input is transformed before the comparison, so the literal never reaches the boundary value.
The honesty corpus independently rejects promotion to `exposed`, even if the
golden report is regenerated.

## Must Not

- Treat a boundary literal that does not reach the changed comparison as a boundary witness.
- Infer a runtime mutation result from static evidence.
