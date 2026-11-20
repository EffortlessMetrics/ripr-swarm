# Fixture: rust_dyn_box_alias_impl

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl Shape for Square { fn len }` changes from `self.0` to `self.0 + 1`.
The workspace declares `pub type DynShape = dyn Shape;` and implements
`Iterator` and `ExactSizeIterator` for `DynShape`. The test binds
`let s: Box<dyn Shape> = Box::new(Square(3));` and asserts `s.len() == 4`.
`Box` forwards `ExactSizeIterator`, so the call runs
`ExactSizeIterator::len`, and the test passes on the old code and on the
new code.

## When

`cargo xtask fixtures rust_dyn_box_alias_impl` runs the public diff
analyzer.

## Then

The `return_value` finding does not read `exposed`. A `type` item that
mentions `dyn` refuses the `Box<dyn Shape>` receiver reading, since an
alias can name the trait object in an `impl` header that never spells
`dyn`.

## Must Not

- Credit `Square`'s `len` because no `impl` header spells `dyn Shape`.
