# Fixture: rust_returned_type_drop_exit

Spec: RIPR-SPEC-0197

## Given

The owner returns `Cfg { n: x * 3 }`. The diff changes the constructed field from `x * 2` to `x * 3`. The only test compares the whole returned value: `assert_eq!(make(4), Cfg { n: 12 })`.

`Cfg` has a `Drop` impl whose body exits the process. When the assertion fails and unwinding drops the returned value, the test binary exits 0, so the test cannot fail.

## When

`cargo xtask fixtures rust_returned_type_drop_exit` runs the public analyzer.

## Then

The field-construction finding must not read `exposed`. It reads `weakly_exposed`. (The return_value finding on the same line is credited through the generic oracle path and remains a disclosed residual on #8480.)

## Must Not

- Credit a whole-value owner pin when the returned type has a non-plain `Drop` impl in the workspace.
