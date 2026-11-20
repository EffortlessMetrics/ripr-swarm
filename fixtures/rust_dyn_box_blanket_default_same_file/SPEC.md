# Fixture: rust_dyn_box_blanket_default_same_file

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl Shape for Square { fn area }` changes from `self.0 * self.0` to
`self.0 * self.0 + 1`. The same file declares `pub trait Ext { fn
area(&self) -> u32 { 7 } }` and the blanket `impl<T: ?Sized> Ext for T {}`.
The test binds `let s: Box<dyn Shape> = Box::new(Square(3));` and asserts
`s.area() == 7`. Method lookup finds `Ext::area` for `Box<dyn Shape>` at
the autoref step, before the vtable, so the test passes on the old code and
on the new code.

## When

`cargo xtask fixtures rust_dyn_box_blanket_default_same_file` runs the
public diff analyzer.

## Then

The `return_value` finding does not read `exposed` or `weakly_exposed`, and
the related test is not a `direct_owner_call`. A second `fn area` outside
`trait Shape` and its one impl refuses the `Box<dyn Shape>` receiver
reading on both the pin and the relation side.

## Must Not

- Credit `Square`'s `area` because the blanket impl names neither `Box`,
  `dyn` nor `Shape`.
