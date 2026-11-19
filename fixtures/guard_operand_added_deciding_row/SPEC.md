# Fixture: guard_operand_added_deciding_row

Spec: RIPR-SPEC-0197

## Given

`is_even` returned `n % 2 == 0` and the diff adds an `n != 0` guard in front.
The related test runs a constant-row table `(4, true), (0, false)` through
`assert_eq!(is_even(n), want)`.

## When

The exact diff is analyzed.

## Then

The return-value finding reads `exposed`: the `(4, true)` row evaluates every
operand, and the `(0, false)` row is an input where the added `n != 0` decides
(the old version returned `true` there), so the guard-operand agreement rule
(#8361, short-circuit tails item 5) does not withhold the pin. This is the
positive control for `guard_operand_dropped_agreement`; the function is total,
so the table also runs.

## Must Not

- Withhold the pin when some run of the pinned call makes the added or dropped
  operand decide.
