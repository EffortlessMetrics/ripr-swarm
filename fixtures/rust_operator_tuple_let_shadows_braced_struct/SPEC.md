# Fixture: rust_operator_tuple_let_shadows_braced_struct

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15801

## Given

`impl Add for Celsius { fn add }` changes, and `impl Add for Kelvin` also
exists. `Celsius` is a braced struct, so it lives only in the type
namespace, and the test binds that name inside a tuple pattern:
`let (_, Celsius) = (1, Kelvin as fn(i32) -> Kelvin);`. `Celsius(1)` then
calls the local value and yields a `Kelvin`, so
`assert_eq!(Celsius(1) + Celsius(2), Celsius(3))` runs the `Kelvin` impl.
The test passes on the old code and on the new.

## When

`cargo xtask fixtures rust_operator_tuple_let_shadows_braced_struct` runs the public diff analyzer.

## Then

Neither finding reads `exposed`, and reach is not established: the test
is related only by file. Before #15801 the `return_value` finding read
`exposed`, because only a plain `let Celsius` refused the reading.

## Must Not

- Read `Celsius(1) + Celsius(2)` as an addition of two `Celsius` values
  because the binder sits inside a tuple pattern.
