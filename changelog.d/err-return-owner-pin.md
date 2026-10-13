<!-- section: Fixed -->
- `assert_eq!(check(20), Err(String::from("big")))` now pins a changed early
  `return Err(..);` the way it already pinned a changed `return Ok(..)`, so a
  change it catches reads `exposed` instead of an actionable gap. An owner
  file that rebinds `Ok`, `Err`, `Some` or `None` as a value refuses an early
  `return Err(..)` / `return None` pin too (RIPR-SPEC-0197, RIPR-SPEC-0231
  example 5).
