# Fixture: rust_into_from_other_target

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl From<Celsius> for Fahrenheit { fn from }` changes, and `Kelvin`
also implements `From<Celsius>`. The test binds
`let k: Kelvin = Celsius(100).into();` and asserts `k == Kelvin(373)`. It
passes on the old code and on the new code, because it runs the `Kelvin`
impl.

## When

`cargo xtask fixtures rust_into_from_other_target` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` and keeps its
file-only relation. The annotation picks the target, so only a
`Fahrenheit` annotation reaches the changed impl.

## Must Not

- Credit the `Fahrenheit` impl because the receiver is a `Celsius`.
