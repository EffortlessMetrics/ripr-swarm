# Fixture: rust_into_from_cfg_gated_owner

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

The changed `impl From<Celsius> for Fahrenheit` is gated
`#[cfg(not(test))]`, and a `#[cfg(test)] impl From<crate::Celsius> for
Fahrenheit` takes its place in the test build. The test binds
`let f: Fahrenheit = Celsius(100).into();` and asserts
`f == Fahrenheit(212)`. It passes on the old code and on the new code,
because the changed impl is not compiled into the test.

## When

`cargo xtask fixtures rust_into_from_cfg_gated_owner` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` and keeps its
file-only relation. A `cfg` on the owner impl refuses the reading, and so
does a second `From` impl for the same pair however its path is spelled.

## Must Not

- Credit the changed impl because the other impl spells its source
  type `crate::Celsius`.
