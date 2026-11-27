# Fixture: rust_into_from_const_shadows_unit

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl From<Boiling> for Fahrenheit { fn from }` changes for the unit
struct `Boiling`, and `impl From<Kelvin> for Fahrenheit` also exists. The
test module declares `const Boiling: Kelvin = Kelvin(373);`. The test
binds `let f: Fahrenheit = Boiling.into();` and asserts
`f == Fahrenheit(212)`. It passes on the old code and on the new code,
because `Boiling` there is the `Kelvin` constant.

## When

`cargo xtask fixtures rust_into_from_const_shadows_unit` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` and keeps its
file-only relation. The constant shadows the unit struct's value.

## Must Not

- Read `Boiling` as the unit struct because a `struct Boiling;` is
  declared.
