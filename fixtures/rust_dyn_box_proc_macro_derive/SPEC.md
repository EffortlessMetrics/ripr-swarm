# Fixture: rust_dyn_box_proc_macro_derive

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl Shape for Square { fn area }` in the `probe` member changes from
`self.0 * self.0` to `self.0 * self.0 + 1`. The `mac` member is a
proc-macro crate whose `Ext` derive emits `pub trait Ext { fn area(..) }
impl Ext for Box<dyn crate::Shape> { fn area(..) { 7 } }` from a string
literal, and `probe` applies `#[derive(Ext)]` to an unrelated struct. The
test binds `let s: Box<dyn Shape> = Box::new(Square(3));` and asserts
`s.area() == 7`. The generated impl answers the call on the `Box`, so the
test passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_dyn_box_proc_macro_derive` runs the public diff
analyzer.

## Then

The `return_value` finding does not read `exposed`. A proc-macro member or
a derive outside the compiler's built-ins refuses the `Box<dyn Shape>`
receiver reading: the expansion is not in any indexed source.

## Must Not

- Credit `Square`'s `area` because the masked sources never spell the
  generated impl.
