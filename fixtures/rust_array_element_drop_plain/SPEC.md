# Fixture: rust_array_element_drop_plain

Spec: RIPR-SPEC-0225

## Given

The owner returns `[Row { total: 0 }, Row { total: a + b }]`. The diff changes the second element's field from `a - b` to `a + b`. The only test pins the second element by index: `assert_eq!(rows[1].total, 33)`.

`Row` has a `Drop` impl with a comment-only body, which cannot leave the process while unwinding, so the test can still fail. This is the positive control for the #8084 Drop refusal.

## When

`cargo xtask fixtures rust_array_element_drop_plain` runs the public analyzer.

## Then

The field-construction finding reads `exposed`.

## Must Not

- Downgrade an indexed element pin whose element type has only a plain (empty) `Drop` impl.
