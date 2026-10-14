<!-- section: Fixed -->
- `assert!(matches!(check(5), Ok(5)))` and `Some(x @ 3)` now pin the owner's
  return value the way `assert_eq!` does, so a change they catch reads
  `exposed` instead of an actionable gap. Only literal patterns qualify:
  `_`, ranges, `|`, guards, paths and constants still refuse. A changed early
  `return Some(..)` (or `Ok(..)`) beside `None` (or `Err(..)`) exits is now an
  established return path too (RIPR-SPEC-0197, RIPR-SPEC-0231 examples 6 and
  23; #7581).
