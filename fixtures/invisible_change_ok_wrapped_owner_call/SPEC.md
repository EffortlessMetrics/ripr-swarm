# Fixture: invisible_change_ok_wrapped_owner_call

Spec: RIPR-SPEC-0197

## Given

`fee` returns `Ok(x * 2)` and the diff changes it to `Ok(x * 3)`. The related
tests call the owner at `0` through its crate path: one asserts
`assert_eq!(..::fee(0), Ok(0))`, the other `assert!(matches!(..::fee(0), Ok(0)))`.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: at `x = 0` both versions return
`Ok(0)`, so neither assertion can notice the change. The shared `Ok(..)`
constructor is read through, and the literal `matches!` pin reads like its
`assert_eq!` twin (#15414).

## Must Not

- Promote the change to `exposed` because the changed value sits inside
  `Ok(..)` or the assertion is spelled as a literal `matches!` pin.
