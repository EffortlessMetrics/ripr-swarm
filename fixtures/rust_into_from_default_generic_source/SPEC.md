# Fixture: rust_into_from_default_generic_source

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`Celsius` is declared `pub struct Celsius<T = i32>(pub T);`. The changed
`impl From<Celsius> for Fahrenheit` converts the default `Celsius<i32>`,
and `impl From<Celsius<u8>> for Fahrenheit` converts another instance. The
test binds `let f: Fahrenheit = Celsius(100u8).into();` and asserts
`f == Fahrenheit(212)`. It passes on the old code and on the new code,
because it runs the `Celsius<u8>` impl.

## When

`cargo xtask fixtures rust_into_from_default_generic_source` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` and keeps its
file-only relation. A source or target type with any generic parameter
refuses the `.into()` reading.

## Must Not

- Credit the `From<Celsius>` impl because the receiver is spelled
  `Celsius(..)`.
