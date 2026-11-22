# Fixture: rust_operator_closure_param_shadows_braced_struct

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15801

## Given

`impl Add for Celsius { fn add }` changes, and `impl Add for Kelvin` also
exists. `Celsius` is a braced struct, so it lives only in the type
namespace, and the test's closure takes a parameter of that name:
`|Celsius: fn(i32) -> Kelvin| { assert_eq!(Celsius(1) + Celsius(2), Kelvin(3)); }`.
Inside the closure `Celsius(1)` calls the parameter and yields a `Kelvin`,
so the `+` runs the `Kelvin` impl. The test passes on the old code and on
the new.

## When

`cargo xtask fixtures rust_operator_closure_param_shadows_braced_struct` runs the public diff analyzer.

## Then

Neither finding reads `exposed`, and reach is not established: the test
is related only by file. A closure parameter named like the impl type
shadows its call spelling and refuses the operator reading, like a `fn`,
`const`, `static` or `let` of that name.

## Must Not

- Read `Celsius(1) + Celsius(2)` as an addition of two `Celsius` values
  because the type namespace still holds the struct.
