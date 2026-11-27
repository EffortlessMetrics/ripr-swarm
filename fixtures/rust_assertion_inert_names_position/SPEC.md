# Fixture: rust_assertion_inert_names_position

Spec: RIPR-SPEC-0224

## Given

`weight(x)` returns `x * 3`, changed to `x * 4`. `in_scrutinee` runs `assert_eq!(weight(4), 12)` inside the scrutinee block of a `match`. `in_condition` runs it inside the condition block of an `if`.

## When

`cargo xtask fixtures rust_assertion_inert_names_position` runs the public analyzer.

## Then

Both rows read `assertion_inert`, and each names the position its assertion sits in: "inside the scrutinee of a `match`" and "inside the condition of an `if`" (#15337 follow-up).

## Must Not

- Name a scrutinee assertion "a `match` arm" or a condition assertion "an `if` branch".
- Change which assertions are credited.
