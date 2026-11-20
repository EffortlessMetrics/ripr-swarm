# Fixture: rust_assertion_inert_names_construct

Spec: RIPR-SPEC-0224

## Given

`weight(x)` returns `x * 3`, changed to `x * 4`. `weighs_in_a_loop` runs `assert_eq!(weight(4), 12)` inside `for _ in 0..cases()`. `weighs_in_two_shapes` runs one assertion inside an `if` and another inside a `while`.

## When

`cargo xtask fixtures rust_assertion_inert_names_construct` runs the public analyzer.

## Then

Both rows read `assertion_inert`. The `weighs_in_a_loop` row names its construct: "this test's assertion is inside a `for` loop, which may run zero times" (#15337). The `weighs_in_two_shapes` row keeps the generic sentence, because its two refusals name different constructs and the row cannot say which one applies.

## Must Not

- Name a construct on a row whose refusals disagree.
- Change the finding's class, confidence or any row's `miss` token.
