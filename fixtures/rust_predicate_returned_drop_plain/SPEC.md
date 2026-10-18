# Fixture: rust_predicate_returned_drop_plain

Spec: RIPR-SPEC-0197

## Given

The owner pre-binds `low = Cfg { n: 2 }` and `high = Cfg { n: 1 }` and returns `high` when `x >= 10`, else `low` (the branch-value shape the predicate flow sink admits). The diff changes the branch condition from `x > 10` to `x >= 10`. The tests pin the flipped boundary and a far point: `assert_eq!(make(10), Cfg { n: 1 })`, `assert_eq!(make(11), Cfg { n: 1 })`.

`Cfg` has a `Drop` impl whose body is only a comment. Dropping the returned value while unwinding cannot exit or observe anything, so the assertion can still fail the test.

## When

`cargo xtask fixtures rust_predicate_returned_drop_plain` runs the public analyzer.

## Then

The predicate finding reads `exposed`: a plain Drop keeps the boundary credit. (The branch values are pre-bound identifiers, the shape the predicate flow sink admits.)

## Must Not

- Downgrade a predicate-boundary finding whose returned type has only a plain (empty or comment-only) Drop impl.
