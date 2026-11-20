<!-- section: Fixed -->
- The repo seam view (`agent brief`, `repair`, repo exposure, editor hover)
  no longer calls a match arm strongly gripped just because a related test
  calls its function. A call runs the `match`, not every arm, so an arm now
  needs a related test whose input is read as selecting it; otherwise its
  activation reads unknown. Every arm of a `match` on a value computed inside
  the function (`match (a.parse::<u64>(), b.parse::<u64>())`) used to read
  strongly gripped with no test reaching it (#15359).
