# Fixture: invisible_change_equal_arguments_swapped_arms

Spec: RIPR-SPEC-0197

## Given

`largest` returned `if a > b { a } else { b }` and the diff swaps the arms.
The only related test asserts `largest(2, 2) == 2`.

## When

The exact diff is analyzed.

## Then

The predicate finding is not `exposed`: with equal arguments either arm
returns `2` on either version. The arguments sit on the `a > b` boundary, but
the boundary input cannot notice this change (#15272).

## Must Not

- Promote the change to `exposed` because a test input sits on the changed
  line's comparison boundary.
