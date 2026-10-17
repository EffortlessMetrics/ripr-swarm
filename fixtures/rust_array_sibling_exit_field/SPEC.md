# Fixture: rust_array_sibling_exit_field

Spec: RIPR-SPEC-0225

## Given

The owner returns `[Row { total: { std::process::exit(0) } }, Row { total: a + b }]`. The diff changes the second element's field from `a - b` to `a + b`. The only test pins the second element by index: `assert_eq!(rows[1].total, 33)`.

Constructing the first element exits the process before the test can reach its assertion, so the test cannot fail.

## When

`cargo xtask fixtures rust_array_sibling_exit_field` runs the public analyzer.

## Then

The field-construction finding must not read `exposed`. It reads `weakly_exposed`.

## Must Not

- Credit an indexed element pin when a sibling element's field may leave the process during construction.
