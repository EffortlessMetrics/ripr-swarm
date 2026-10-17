# Fixture: rust_returned_type_drop_plain

Spec: RIPR-SPEC-0197

## Given

The owner returns `Cfg { n: x * 3 }`. The diff changes the constructed field from `x * 2` to `x * 3`. The only test compares the whole returned value: `assert_eq!(make(4), Cfg { n: 12 })`.

`Cfg` has a `Drop` impl with a comment-only body, which cannot leave the process while unwinding, so the test can still fail. This is the positive control for the #8480 Drop refusal.

## When

`cargo xtask fixtures rust_returned_type_drop_plain` runs the public analyzer.

## Then

The field-construction finding reads `exposed`.

## Must Not

- Downgrade a whole-value owner pin whose returned type has only a plain (empty) `Drop` impl.
