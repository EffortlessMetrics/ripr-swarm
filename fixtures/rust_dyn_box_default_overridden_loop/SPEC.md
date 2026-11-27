# Fixture: rust_dyn_box_default_overridden_loop

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15752

## Given

The trait default `Shape::area` changes from `self.base() * 2` to
`self.base() * 2 + 1`. `impl Shape for Square` overrides `area`. The test
builds `let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Square(3))];` and
asserts `s.area() == 7` for each `s` in `&shapes`. It passes on the old
code and on the new code, because each call runs `Square`'s override,
never the default.

## When

`cargo xtask fixtures rust_dyn_box_default_overridden_loop` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` or `weakly_exposed`, and
the related test is not a `direct_owner_call`. The loop variable is bound
by a `for` pattern, not a `let`, so its boxed type is unknown, and an
override of `area` exists.

## Must Not

- Credit the changed default because the receiver is bound by a `for`
  pattern that ripr does not tie to a boxed type.
