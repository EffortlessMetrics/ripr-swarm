# Fixture: rust_returned_error_drop_plain

Spec: RIPR-SPEC-0197

## Given

The owner returns `Err(Cfg { n: x * 3 })`. The diff changes the constructed field from `x * 2` to `x * 3`. The only test compares the whole returned error: `assert_eq!(parse(4), Err(Cfg { n: 12 }))`.

`Cfg` has a `Drop` impl whose body is only a comment. Dropping the returned error value while unwinding cannot exit or observe anything, so the assertion can still fail the test.

## When

`cargo xtask fixtures rust_returned_error_drop_plain` runs the public analyzer.

## Then

The error-path finding reads `exposed`: a plain Drop keeps the whole-value error credit.

## Must Not

- Downgrade an error-path finding whose returned error type has only a plain (empty or comment-only) Drop impl.
