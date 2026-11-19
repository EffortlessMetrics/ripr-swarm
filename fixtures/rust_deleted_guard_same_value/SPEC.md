# Fixture: rust_deleted_guard_same_value

Spec: RIPR-SPEC-0197

## Given

Two functions each lose an `if x == 0 { return Vec::new(); }` early return
(#14936). Both keep their constant tail:

- `pick` returns `vec![]`, the same value the deleted guard returned.
- `pick_nine` returns `vec![9]`, a different value.

One test per function pins the guard's input with
`assert_eq!(f(0), Vec::<u32>::new())`.

## When

`cargo xtask fixtures rust_deleted_guard_same_value` runs the public
analyzer.

## Then

- The deleted guard line of `pick` reads its `return_value` probe as
  `weakly_exposed`: `pick(0)` returns an empty vector on both sides, so the
  pin cannot tell the guard's deletion apart. The discriminate stage names
  `deleted_guard_same_value`.
- The deleted guard line of `pick_nine` reads its `return_value` probe as
  `exposed`: `pick_nine(0)` returned an empty vector on the base side and
  returns `[9]` on the candidate side.

## Must Not

- Read the same-value deletion as `exposed`.
- Withhold credit from a deleted guard whose remaining tail returns a
  different value.
