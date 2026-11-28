# Fixture: typescript_vitest_compared_owner_call_boundary

Spec: RIPR-SPEC-0224

Additional contract: RIPR-SPEC-0027; #16496.

## Given

Two Vitest tests import `discount`. The edit changes `total > 100` to
`total >= 100` in `discount`. Neither test asserts the owner call that
receives the boundary input `100`:

- `expect(discount(100) > 0).toBe(true)` asserts a comparison, which is `true`
  on both sides of the boundary.
- `expect(discount(50) /* discount(100) */ + 40).toBe(90)` spells the boundary
  input only inside a comment; the call that runs takes `50`.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding stays `weakly_exposed`. The boundary input reaches the owner in
the first test, but the asserted value is not the owner result, so the
assertion does not witness the changed comparison. The commented call is not
a call and supplies no input.

The honesty corpus independently rejects promotion to `exposed`, even if the
golden report is regenerated. The adapter tests in
`free_function_whole_call_tests.rs` pin the row activations, and keep the
whole call, a one-hop local, parentheses, `await`, `Promise.resolve` and a
namespace member call witnessed.

## Must Not

- Credit a boundary input whose owner call sits inside a comparison,
  arithmetic, a negation or a template.
- Read an owner call from a comment.
- Infer a runtime mutation result from static evidence.
