<!-- section: Fixed -->
- The repo seam view (`agent brief`, `repair`, repo exposure, editor hover)
  no longer calls a match arm strongly gripped just because a related test
  calls its function. A call runs the `match`, not every arm, so an arm now
  needs a related test whose input is read as selecting it (a literal, a
  constant table row, or a typed receiver for a method). Without one, its
  activation reads unknown. When the shared arm reading establishes that
  every related test calls the function with inputs selecting other arms,
  activation reads weak instead and the arm's pattern is named as the
  missing discriminator. Every arm of a `match` on a value computed inside
  the function (`match (a.parse::<u64>(), b.parse::<u64>())`) used to read
  strongly gripped with no test reaching it (#15359).
