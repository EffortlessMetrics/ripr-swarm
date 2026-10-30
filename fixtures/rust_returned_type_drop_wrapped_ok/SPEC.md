# Fixture: rust_returned_type_drop_wrapped_ok

Spec: RIPR-SPEC-0197

## Given

The owner returns `Ok(Cfg { n: x * 3 })`. The diff changes the constructed field from `x * 2` to `x * 3`. The only test compares the whole returned value: `assert_eq!(make(4), Ok(Cfg { n: 12 }))`.

`Cfg` has a `Drop` impl whose body exits the process. When the assertion fails and unwinding drops the returned value, the test binary exits 0, so the test cannot fail.

## When

`cargo xtask fixtures rust_returned_type_drop_wrapped_ok` runs the public analyzer.

## Then

The finding on the changed line must not read `exposed`; it reads `weakly_exposed`. A record wrapped in a call argument in exit position is still an exit-position construction.

## Must Not

- Credit a whole-value assertion when a record type constructed inside an exit-position wrapper has a non-plain `Drop` impl in the workspace.
