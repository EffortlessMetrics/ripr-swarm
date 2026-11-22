# Fixture: invisible_change_tuple_agreeing_component

Spec: RIPR-SPEC-0197

## Given

`pair` returned `(a, a > 0)` and the diff changes the second component to
`a >= 0`. The only related test asserts `pair(5) == (5, true)`.

## When

The exact diff is analyzed.

## Then

The return-value finding is not `exposed`: `pair(5)` is `(5, true)` on both
versions, so the whole-tuple assertion cannot notice the change. The tuple
tail is folded one component at a time (#15272).

## Must Not

- Promote the change to `exposed` from an exact tuple assertion at an input
  where both versions build the same tuple.
