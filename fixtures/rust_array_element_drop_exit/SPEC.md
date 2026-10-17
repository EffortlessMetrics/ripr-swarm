# Fixture: rust_array_element_drop_exit

Spec: RIPR-SPEC-0225

## Given

The owner returns `[Row { total: 0 }, Row { total: a + b }]`. The diff changes the second element's field from `a - b` to `a + b`. The only test pins the second element by index: `assert_eq!(rows[1].total, 33)`.

`Row` has a `Drop` impl whose body exits the process. When the assertion fails and unwinding drops the receiver, the test binary exits 0, so the test cannot fail.

## When

`cargo xtask fixtures rust_array_element_drop_exit` runs the public analyzer.

## Then

The field-construction finding must not read `exposed`. It reads `weakly_exposed`. The return_value finding on the returned array reads `weakly_exposed` on the same evidence: the returned array is alive in the asserting test, so unwinding drops the same elements.

## Must Not

- Credit an indexed element pin when the element type has a non-plain `Drop` impl in the workspace.
