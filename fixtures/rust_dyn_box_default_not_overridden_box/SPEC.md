# Fixture: rust_dyn_box_default_not_overridden_box

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15752

## Given

The trait default `Shape::area` changes from `self.base() * 2` to
`self.base() * 2 + 1`. `impl Shape for Square` overrides `area`;
`impl Shape for Circle` does not. The test binds
`let s: Box<dyn Shape> = Box::new(Circle(3));` and asserts
`s.area() == 7`. It passes on the new code and fails on the old code,
because the call runs the changed default through `Circle`'s vtable.

## When

`cargo xtask fixtures rust_dyn_box_default_not_overridden_box` runs the public diff analyzer.

## Then

The `return_value` finding reads `weakly_exposed`, and the related test
keeps its `direct_owner_call` relation. The override refusal reads the
boxed value as `Circle`, which no override of `area` is for.

## Must Not

- Refuse the changed default because some other impl of `Shape`
  (`Square`) overrides `area`.
