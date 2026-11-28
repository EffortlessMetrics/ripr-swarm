# Fixture: typescript_vitest_class_method_boundary

Spec: RIPR-SPEC-0224

Additional contract: #4102; #15950; #16482.

## Given

`src/cart.ts` exports a class `Cart` with a method `discount`. The edit
changes `total > 100` to `total >= 100` in `discount`. A Vitest test imports
`Cart` and calls the method on an instance.

- The first test binds `const cart = new Cart()` and pins the exact boundary
  input, `expect(cart.discount(100)).toBe(90)`.
- The second test calls the method on an inline `new Cart()` with an
  off-boundary input, `expect(new Cart().discount(50)).toBe(50)`.

## When

Analyze the committed change in `diff.patch` with the TypeScript adapter.

## Then

The finding is `exposed`. The test reaches the method through a receiver the
relation resolves, and the boundary witness reads the same receivers: a local
bound to `new Cart()` that is never rebound, or an inline `new Cart()`.
`cart.discount(100)` sits on the changed boundary, so the pinned assertion
discriminates the edit.

The honesty corpus keeps this control at `exposed`. The adapter tests in
`method_receiver_witness_tests.rs` pin the row activations, including an
off-boundary input, a self-comparing expectation, a boundary input passed to
the constructor, a same-named method on another receiver or class, and a
rebound, replaced, spied or shadowed method.

## Must Not

- Credit a same-named method called on a receiver the relation does not
  resolve to the owner's class.
- Credit a method the test rebinds, replaces, spies on or mocks.
- Infer a runtime mutation result from static evidence.
