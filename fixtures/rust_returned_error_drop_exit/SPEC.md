# Fixture: rust_returned_error_drop_exit

Spec: RIPR-SPEC-0197

## Given

The owner returns `Err(Cfg { n: x * 3 })`. The diff changes the constructed field from `x * 2` to `x * 3`. The only test compares the whole returned error: `assert_eq!(parse(4), Err(Cfg { n: 12 }))`.

`Cfg` has a `Drop` impl whose body exits the process. When the assertion fails and unwinding drops the returned error value, the test binary exits 0, so the test cannot fail.

## When

`cargo xtask fixtures rust_returned_error_drop_exit` runs the public analyzer.

## Then

The error-path finding must not read `exposed`. It reads `weakly_exposed`: the generic oracle path credits the whole-value error assertion without consulting the returned type's Drop, so the demotion happens at the final semantic owner on the same evidence as the return_value demotion.

## Must Not

- Credit an error-path assertion when the returned error type has a non-plain `Drop` impl in the workspace.
