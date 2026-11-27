<!-- section: Changed -->
- An `assertion inert` row for an assertion inside an `if` or `while`
  condition, a `match` scrutinee or a `for` iterable now names that position
  ("inside the scrutinee of a `match`") instead of calling it a branch, arm
  or loop body that may be skipped (#15337 follow-up).
