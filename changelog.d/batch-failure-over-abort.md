<!-- section: Fixed -->
- When a parallel indexing batch has both an ordinary file failure (for
  example an unreadable `.rs` file) and an abort that a sibling file observed,
  the editor refresh and `ripr review-comments` now report the file failure
  instead of the deadline or cancellation. The first ordinary failure in input
  order wins regardless of which worker reached a checkpoint first, so a
  persistent source failure is no longer hidden behind a recurring deadline
  (#6721).
