# Fixture: rust_dyn_box_unindexed_member_blanket

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl Shape for Square { fn area }` in the `app` member changes from
`self.0 * self.0` to `self.0 * self.0 + 1`. The unchanged `extlib` member,
which `app` depends on and imports `Ext` from, declares `pub trait Ext {
fn area(&self) -> u32 { 7 } }` and the blanket `impl<T: ?Sized> Ext for T
{}`. The test binds `let s: Box<dyn Shape> = Box::new(Square(3));` and
asserts `s.area() == 7`. The blanket impl answers the call on the `Box`, so
the test passes on the old code and on the new code.

## When

`cargo xtask fixtures rust_dyn_box_unindexed_member_blanket` runs the
public diff analyzer.

## Then

The `return_value` finding does not read `exposed` or `weakly_exposed` in
any mode. A workspace member the mode does not index refuses the
`Box<dyn Shape>` receiver reading; when the member is indexed, its second
`fn area` refuses it.

## Must Not

- Credit `Square`'s `area` because the draft index holds only the changed
  member.
