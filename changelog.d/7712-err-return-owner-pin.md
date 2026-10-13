<!-- section: Fixed -->
- `assert_eq!(check(20), Err(String::from("big")))` now pins a changed early
  `return Err(..);` the way it already pinned a changed `return Ok(..)`, so a
  change it catches reads `exposed` instead of an actionable gap. The pin is
  refused when the owner's file binds `Ok` or `Err` as a value, or when the
  test file declares or imports the expected constructor's name. A bare
  `None =>` match arm is no longer read as such a binding, so it stops
  refusing an early `return Some(..)` pin (RIPR-SPEC-0197, RIPR-SPEC-0231
  example 5; #7712).
