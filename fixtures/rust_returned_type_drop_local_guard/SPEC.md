# Fixture: rust_returned_type_drop_local_guard

Spec: RIPR-SPEC-0197

## Given

The owner constructs `Guard { n: 0 }` only in a local, non-returned position and returns the scalar `x * 3`. The diff changes the returned expression from `x * 2` to `x * 3`. The only test compares the returned scalar exactly: `assert_eq!(compute(4), 12)`.

`Guard` has a non-plain `Drop` impl (a non-empty body that updates its own field), but the guard value is dropped inside the owner, not while unwinding from the test's failed assertion, and it is never the returned value. This gate judges the returned value's `Drop`; a local value whose `Drop` body itself may exit is an owner-side exit question outside this gate, with the same posture as other owner-side exits for `return_value`.

## When

`cargo xtask fixtures rust_returned_type_drop_local_guard` runs the public analyzer.

## Then

The return_value finding on the changed line reads `exposed`. A record constructed only in a non-exit position must not demote the finding.

## Must Not

- Demote a finding because a non-plain Drop record is constructed only locally and never becomes the function result.
