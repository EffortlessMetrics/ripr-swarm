# Fixture: typescript_vitest_sibling_expected_side_boundary

Spec: RIPR-SPEC-0224

Additional contract: RIPR-SPEC-0027; #4102; #16458.

## Given

Two Vitest tests import `discount`. The edit changes `total > 100` to
`total >= 100` in `discount`.

- The first test pins the boundary with `expect(discount(100)).toBe(100)`. A
  sibling assertion with the same observed call compares it against another
  owner call: `expect(discount(100)).toBeLessThan(discount(200))`.
- The second test pins an off-boundary input and adds a self-comparing
  tautology, `expect(discount(150)).toBe(discount(150))`.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding is `exposed`. The first assertion's own expected side is the
literal `100`, which the boundary input reaches on one side of the changed
comparison only. The sibling's owner-calling matcher belongs to the sibling,
so it does not veto the pinned assertion. The tautology still never
witnesses: its own expected side calls the owner.

The honesty corpus keeps this control at `exposed`. The adapter test
`only_the_assertions_own_expected_side_vetoes_its_witness` pins the row
activations, including a negative row whose own expected side calls the
owner.

## Must Not

- Let one assertion's matcher arguments withhold another assertion's witness.
- Credit an assertion whose own expected side calls the owner.
- Infer a runtime mutation result from static evidence.
