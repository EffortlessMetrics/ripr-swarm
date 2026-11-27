# Fixture: typescript_vitest_wrapped_owner_call_boundary

Spec: RIPR-SPEC-0224

Additional contract: RIPR-SPEC-0027; #16458.

## Given

Two Vitest tests import `discount`. The edit changes `total > 100` to
`total >= 100` in `discount`. Each test passes the boundary input `100` to the
owner, but neither asserts that call's own result:

- `expect(Math.max(discount(100), 100)).toBe(100)` asserts a wrapper's result,
  which is `100` on both sides of the boundary.
- `expect(discount(90, discount(100))).toBe(90)` asserts the outer call, whose
  first argument `90` is off the boundary.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding stays `weakly_exposed`. The boundary input reaches the owner, but
no assertion observes the owner call that receives it, so neither assertion
witnesses the changed comparison.

The honesty corpus independently rejects promotion to `exposed`, even if the
golden report is regenerated. The adapter test
`wrapped_or_nested_owner_call_does_not_witness` pins the row activations and
keeps a whole owner call witnessed beside a wrapped sibling assertion.

## Must Not

- Credit a boundary input that reaches the owner only inside a wrapper call
  or a nested owner call.
- Infer a runtime mutation result from static evidence.
