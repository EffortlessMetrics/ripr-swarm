# Fixture: rust_dyn_box_trait_impl

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl Shape for Square { fn area }` changes from `self.0 * self.0` to
`self.0 * self.0 + 1`. The test binds `let s: Box<dyn Shape> =
Box::new(Square(3));` and asserts `s.area() == 10`. The test passes on the
new code and fails on the old code.

## When

`cargo xtask fixtures rust_dyn_box_trait_impl` runs the public diff analyzer.

## Then

The `return_value` finding reads `exposed` with a `direct_owner_call`
relation. A `Trait` method called through `Box<dyn Trait>` runs the boxed
value's impl, the same body `Square(3).area()` runs.

## Must Not

- Read the call as name-only (`weak_token_substring`) because the
  annotation's head is `Box`.
