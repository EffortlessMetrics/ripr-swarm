# Fixture: rust_into_from_reference_binding

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl From<Celsius> for Fahrenheit { fn from }` changes, and
`impl From<&Celsius> for Fahrenheit` also exists. The test binds
`let c = &Celsius(100);` and `let f: Fahrenheit = c.into();` and asserts
`f == Fahrenheit(212)`. It passes on the old code and on the new code,
because `c` is a `&Celsius` and the call runs the `From<&Celsius>` impl.

## When

`cargo xtask fixtures rust_into_from_reference_binding` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` and keeps its
file-only relation. A receiver written with `&` is not exactly a
`Celsius`.

## Must Not

- Credit the `From<Celsius>` impl because the binding's initializer
  names `Celsius`.
