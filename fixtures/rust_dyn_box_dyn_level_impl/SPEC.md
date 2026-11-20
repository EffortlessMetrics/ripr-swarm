# Fixture: rust_dyn_box_dyn_level_impl

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl Shape for Square { fn len }` changes from `self.0` to `self.0 + 1`.
The workspace also implements `Iterator` and `ExactSizeIterator` for
`dyn Shape`. The test binds `let s: Box<dyn Shape> = Box::new(Square(3));`
and asserts `s.len() == 4`. `Box` forwards `ExactSizeIterator`, so the call
runs `ExactSizeIterator::len`, and the test passes on the old code and on
the new code.

## When

`cargo xtask fixtures rust_dyn_box_dyn_level_impl` runs the public diff
analyzer.

## Then

The `return_value` finding does not read `exposed`. An `impl` header that
names `dyn Shape` refuses the `Box<dyn Shape>` receiver reading.

## Must Not

- Credit `Square`'s `len` because the boxed value is a `Square` and no other
  `fn len` is written in the workspace.
