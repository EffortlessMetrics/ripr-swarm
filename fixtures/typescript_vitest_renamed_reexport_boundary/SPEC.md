# Fixture: typescript_vitest_renamed_reexport_boundary

Spec: RIPR-SPEC-0224

Additional contract: RIPR-SPEC-0095; #4102; #16470.

## Given

`src/index.ts` re-exports `discount` from `src/price.ts` under a new name,
`export { discount as applyDiscount } from './price'`. A Vitest test imports that name
through the barrel, `import { applyDiscount } from '../src'`. The edit changes `total > 100` to
`total >= 100` in `discount`.

- The first test pins the exact boundary input,
  `expect(applyDiscount(100)).toBe(90)`.
- The second test pins an off-boundary input,
  `expect(applyDiscount(50)).toBe(50)`.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding is `exposed`. The test reaches the owner through the bounded
re-export chain, and the boundary witness reads the re-exported local
`applyDiscount` as the owner's spelling, with the same shadow guards as the
relation. `applyDiscount(100)` sits on the changed boundary, so the pinned
assertion discriminates the edit.

The honesty corpus keeps this control at `exposed`. The adapter tests in
`reexport_boundary_witness_tests.rs` pin the row activations, including an
off-boundary input, a self-comparing expectation and a destructured shadow.

## Must Not

- Credit a re-exported local that a body-local declaration or destructuring shadows.
- Credit a name the barrel does not export to the owner.
- Infer a runtime mutation result from static evidence.
