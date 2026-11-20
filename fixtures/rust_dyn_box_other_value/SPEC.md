# Fixture: rust_dyn_box_other_value

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl Shape for Square { fn area }` changes, and `Circle` also implements
`Shape`. The test boxes a `Circle` as `Box<dyn Shape>` and asserts
`s.area() == 10`. It passes on the old code and on the new code, because
it never runs `Square`'s impl.

## When

`cargo xtask fixtures rust_dyn_box_other_value` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. The boxed value is a
`Circle`, so the call dispatches to `Circle`'s impl.

## Must Not

- Credit `Square`'s impl because the binding names the shared trait.
