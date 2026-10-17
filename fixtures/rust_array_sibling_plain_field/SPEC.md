# Fixture: rust_array_sibling_plain_field

Spec: RIPR-SPEC-0225

## Given

The owner returns `[Row { total: a }, Row { total: a + b }]`. The diff changes the second element's field from `a - b` to `a + b`. The only test pins the second element by index: `assert_eq!(rows[1].total, 33)`.

The sibling element builds its field from a plain path, which cannot leave the process during construction, so the test reaches its assertion and can fail.

## When

`cargo xtask fixtures rust_array_sibling_plain_field` runs the public analyzer.

## Then

The field-construction finding reads `exposed`. This is the positive control for the #8084 sibling-plainness refusal.

## Must Not

- Downgrade an indexed element pin whose siblings are literals or plain paths.
