# Fixture: rust_into_from_variant_cased_assoc_fn

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl From<Celsius> for Fahrenheit { fn from }` changes, and
`impl From<Kelvin> for Fahrenheit` also exists. `Celsius` declares an
associated function with a variant-cased name, `fn Boiling() -> Kelvin`.
The test binds `let f: Fahrenheit = Celsius::Boiling().into();` and asserts
`f == Fahrenheit(212)`. It passes on the old code and on the new code,
because `Celsius::Boiling()` is a `Kelvin` and the call runs the `Kelvin`
impl.

## When

`cargo xtask fixtures rust_into_from_variant_cased_assoc_fn` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed` and keeps its
file-only relation. A declared associated function decides by its return
type even when its name looks like an enum variant.

## Must Not

- Read `Celsius::Boiling()` as an enum variant constructor of `Celsius`
  because its name is capitalized.
