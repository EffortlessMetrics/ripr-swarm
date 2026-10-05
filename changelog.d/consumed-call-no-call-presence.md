<!-- section: Fixed -->
- Repo seams: a call whose value feeds a condition, binding, operand,
  argument, receiver or return (such as `s.len()` in a guard, `digit(c)` in
  `ok_or`, or a tail `Ok(out)`) no longer emits a `call_presence` seam that
  graded `weakly_gripped` beside exact value assertions and led pilot's top
  picks. Discarded calls keep their `call_presence` seam (#6677).
