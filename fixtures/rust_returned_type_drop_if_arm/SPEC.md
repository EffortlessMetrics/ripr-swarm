# Fixture: rust_returned_type_drop_if_arm

Spec: RIPR-SPEC-0197

## Given

The owner returns `Cfg { n: x * 3 }` from the tail of an `if` branch that feeds the function result. The diff changes the constructed field from `x * 2` to `x * 3`. The only test compares the whole returned value: `assert_eq!(make(true, 4), Cfg { n: 12 })`.

`Cfg` has a `Drop` impl whose body exits the process. When the assertion fails and unwinding drops the returned value, the test binary exits 0, so the test cannot fail.

## When

`cargo xtask fixtures rust_returned_type_drop_if_arm` runs the public analyzer.

## Then

The finding on the changed line must not read `exposed`; it reads `weakly_exposed`. A record in an if-branch tail that feeds an exit position is still an exit-position construction.

## Must Not

- Credit a whole-value assertion when a record type constructed in an exit-feeding branch tail has a non-plain `Drop` impl in the workspace.
