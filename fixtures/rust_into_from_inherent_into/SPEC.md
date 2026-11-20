# Fixture: rust_into_from_inherent_into

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl From<Celsius> for Fahrenheit { fn from }` changes, and `Celsius`
also has an inherent `fn into(self) -> Fahrenheit`. The test binds
`let f: Fahrenheit = Celsius(100).into();` and asserts
`f == Fahrenheit(212)`. It passes on the old code and on the new code,
because the inherent method wins over the blanket `Into` and never runs
the `From` impl.

## When

`cargo xtask fixtures rust_into_from_inherent_into` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` and keeps its
file-only relation. A workspace `fn into` refuses the `.into()` reading.

## Must Not

- Credit the `From` impl because the annotation names its target type.
