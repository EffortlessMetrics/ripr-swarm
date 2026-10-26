# Fixture: rust_diverging_call_error_side

Spec: RIPR-SPEC-0227

## Given

The only assertion is `assert_eq!(total(skip()).ok(), None)`, where `skip()` calls `std::process::exit(0)` (#8015). The test binary ends cleanly before `total` runs.

## When

`ripr check` analyzes the respelled `?` statement (`(digit(c))?` -> `digit(c)?`).

## Then

The finding is not `exposed`: the assertion never runs, so it cannot tell a swallowed `?` apart.

## Must Not

Credit the `?` side flip from an assertion that a diverging call keeps from running.
