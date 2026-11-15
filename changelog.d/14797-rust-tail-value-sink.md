<!-- section: Fixed -->
- Rust: a changed single-line tail value with no call, digit, string or spaced
  arithmetic in it (`false`, `!ready`, `-delta`, `mask & bits`, `vec![]`) now
  reaches the owner's return instead of reading `propagation_unknown`, so an
  exact assertion reads `exposed` and a bare call `reachable_unrevealed`. A
  tail whose text recurs in the function (`-a` to `a` in `fn f(a: i32)`) stays
  unknown (#14797, #14815).
