# Fixture: rust_into_from_doctest_own_types

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl From<Celsius> for Fahrenheit { fn from }` changes in the library. A
doctest on `reading` declares its own `Celsius`, `Fahrenheit` and
`impl From<Celsius> for Fahrenheit`, then binds
`let f: Fahrenheit = Celsius(100).into();` and asserts
`f == Fahrenheit(212)`. A doctest compiles as a separate crate, so it runs
its own impl and never the library's.

## When

`cargo xtask fixtures rust_into_from_doctest_own_types` runs the public diff analyzer.

## Then

The finding does not read `exposed` through the doctest. The `.into()`
reading never relates a doctest to a library `From` impl, because the
library file's declarations say nothing about the doctest's types.

## Must Not

- Credit the library's changed `from` to a doctest that declares its own
  types with the same names.
