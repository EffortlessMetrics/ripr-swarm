# Fixture: invisible_change_wrapped_bool_payload

Spec: RIPR-SPEC-0197

## Given

`check` returns `Some(x >= 0)` and the diff changes it to `Some(x > 0)`. The
related test calls the owner at `5` through its crate path and asserts
`assert!(matches!(..::check(5), Some(true)))`.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: at `x = 5` both versions return
`Some(true)`, so the assertion cannot notice the change. The shared `Some(..)`
constructor is read through to its boolean payload (#16400).

## Must Not

- Promote the change to `exposed` because the changed payload is a boolean.
- Withhold credit from a pin at `x = 0`, where the versions return
  `Some(true)` and `Some(false)` (see `crates/ripr/tests/invisible_change.rs`).
