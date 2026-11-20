# Fixture: rust_dyn_box_trait_default

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The trait default `Counter::advance` changes from `self.step() + 1` to
`self.step() + 2`. `Unit` implements `Counter` without overriding
`advance`. The test binds `let c: Box<dyn Counter> = Box::new(Unit);` and
asserts `c.advance() == 3`. The test passes on the new code and fails on
the old code.

## When

`cargo xtask fixtures rust_dyn_box_trait_default` runs the public diff analyzer.

## Then

The `return_value` finding reads `exposed`. The boxed unit value types the
receiver as `Unit`, whose vtable entry for `advance` is the changed default.

## Must Not

- Leave the pin at `observation_unconfirmed` because the receiver's
  annotation is `Box<dyn Counter>`.
