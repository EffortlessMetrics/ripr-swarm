# Fixture: rust_dyn_box_default_overridden

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15752

## Given

The trait default `Shape::area` changes from `self.base() * 2` to
`self.base() * 2 + 1`. `impl Shape for Square` overrides `area`. The test
binds `let s: Box<dyn Shape> = Box::new(Square(3));` and asserts
`s.area() == 7`. It passes on the old code and on the new code, because
the call runs `Square`'s override, never the default.

## When

`cargo xtask fixtures rust_dyn_box_default_overridden` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` or `weakly_exposed`, and
the related test is not a `direct_owner_call`. The boxed value is a
`Square`, whose vtable entry for `area` is its own override.

## Must Not

- Credit the changed default because the receiver's annotation is
  `Box<dyn Shape>` rather than `Square`.
