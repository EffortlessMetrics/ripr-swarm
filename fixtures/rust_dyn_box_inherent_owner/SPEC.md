# Fixture: rust_dyn_box_inherent_owner

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The inherent `Square::area` changes. `impl Shape for Square` has its own
unchanged `area`. The test boxes a `Square` as `Box<dyn Shape>` and asserts
`s.area() == 10`. It passes on the old code and on the new code, because a
`dyn Shape` call runs only `Shape`'s methods.

## When

`cargo xtask fixtures rust_dyn_box_inherent_owner` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. An inherent method is
not callable through `dyn Shape`.

## Must Not

- Credit the inherent method because the boxed value is a `Square`.
