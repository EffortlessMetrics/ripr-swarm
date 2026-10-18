# Fixture: rust_predicate_returned_drop_exit

Spec: RIPR-SPEC-0197

## Given

The owner pre-binds `low = Cfg { n: 2 }` and `high = Cfg { n: 1 }` and returns `high` when `x >= 10`, else `low` (the branch-value shape the predicate flow sink admits). The diff changes the branch condition from `x > 10` to `x >= 10`. The tests pin the flipped boundary and a far point: `assert_eq!(make(10), Cfg { n: 1 })`, `assert_eq!(make(11), Cfg { n: 1 })`.

`Cfg` has a `Drop` impl whose body exits the process. When the assertion fails and unwinding drops the returned value, the test binary exits 0, so the test cannot fail.

## When

`cargo xtask fixtures rust_predicate_returned_drop_exit` runs the public analyzer.

## Then

The predicate finding must not read `exposed`. It reads `weakly_exposed`: the boundary assertion holds the returned value, and dropping it during unwind may exit before the failure is reported.

## Must Not

- Credit a predicate-boundary assertion when the returned type has a non-plain `Drop` impl in the workspace.
