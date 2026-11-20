# Fixture: rust_dyn_box_path_module

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl Shape for Square { fn area }` changes from `self.0 * self.0` to
`self.0 * self.0 + 1`. The crate loads `#[path = "../../shared/ext.rs"]
mod ext;`, a file outside the analyzed root that declares `pub trait Ext {
fn area(&self) -> u32; }` and implements it for `std::boxed::Box<dyn
crate::Shape>` returning `7`. The test binds `let s: Box<dyn Shape> =
Box::new(Square(3));` and asserts `s.area() == 7`. The impl answers the
call on the `Box`, so the test passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_dyn_box_path_module` runs the public diff
analyzer.

## Then

The `return_value` finding does not read `exposed`. A `#[path]` attribute
anywhere refuses the `Box<dyn Shape>` receiver reading, since it may load
a module the index does not read.

## Must Not

- Credit `Square`'s `area` because no indexed file spells an impl for
  `Box`.
