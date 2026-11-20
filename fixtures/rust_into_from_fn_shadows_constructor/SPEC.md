# Fixture: rust_into_from_fn_shadows_constructor

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl From<Celsius> for Fahrenheit { fn from }` changes, and
`impl From<Kelvin> for Fahrenheit` also exists. The test module declares
`fn Celsius(c: i32) -> Kelvin`, so `Celsius(100)` in the test is a
`Kelvin`. The test binds `let f: Fahrenheit = Celsius(100).into();` and
asserts `f == Fahrenheit(212)`. It passes on the old code and on the new
code, because it runs the `Kelvin` impl.

## When

`cargo xtask fixtures rust_into_from_fn_shadows_constructor` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` and keeps its
file-only relation. A `fn`, `const` or `static` named like the source type
shadows its constructor and refuses the reading.

## Must Not

- Read `Celsius(100)` as a `Celsius` value because the type namespace
  still holds the struct.
