# Fixture: typescript_mock_payload_iife_impl

Spec: RIPR-SPEC-0247
Spec: RIPR-SPEC-0108

## Given

A TypeScript owner changes a call on its `db` parameter. The related test
binds the mock through a wrapper, not a fresh mock:

```ts
const db = { write: jest.fn((() => recordTwo)()) };
```

The immediately invoked arrow evaluates to the named `recordTwo`
implementation, which can record the asserted `toHaveBeenCalledWith(2)`
payload on the mock through `this` even though the owner passed a different
value. `closed_mock_call` must not read the arrow token inside the wrapper
as the implementation (RIPR-SPEC-0247 rule 1, #8138, #8148 review).

## When

```bash
cargo xtask fixtures typescript_mock_payload_iife_impl
```

or:

```bash
ripr check \
  --root fixtures/typescript_mock_payload_iife_impl/input \
  --diff fixtures/typescript_mock_payload_iife_impl/diff.patch \
  --mode fast
```

## Then

The TypeScript adapter:

- finds the `save` owner in `src/save.ts` and the changed `db.write(...)`
  line,
- finds the related test by the owner call,
- does not credit the wrapped-implementation binding as a fresh mock, so
  the payload pin does not promote the finding past `weakly_exposed`.

## Must Not

The `jest.fn((() => recordTwo)())` binding must not read as an
arrow-implementation mock: the finding must not carry a strong
`mock_expectation` oracle for the changed call, and must not promote to
`exposed`. A fresh `jest.fn()` or plain-arrow binding keeps its credit
(covered by `literal_matcher_tests.rs::
mock_payload_binding_must_be_exactly_a_fresh_mock`).
