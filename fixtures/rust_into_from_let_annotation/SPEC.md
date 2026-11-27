# Fixture: rust_into_from_let_annotation

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl From<Celsius> for Fahrenheit { fn from }` changes from
`c.0 * 9 / 5 + 30` to `c.0 * 9 / 5 + 32`. The test binds
`let f: Fahrenheit = Celsius(100).into();` and asserts
`f == Fahrenheit(212)`. The test passes on the new code and fails on the
old code.

## When

`cargo xtask fixtures rust_into_from_let_annotation` runs the public diff analyzer.

## Then

The `return_value` finding reads `exposed` with a `direct_owner_call`
relation. `.into()` on a `Celsius` with a `Fahrenheit` annotation is the
standard blanket `Into`, which runs `Fahrenheit::from`, the same body
`Fahrenheit::from(Celsius(100))` runs.

## Must Not

- Read the test as related only by file (`same_test_file`) because no
  call fact names `from`.
