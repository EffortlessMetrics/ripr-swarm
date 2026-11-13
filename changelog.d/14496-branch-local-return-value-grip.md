<!-- section: Fixed -->
- Repo seam grip: a branch-local `return_value` seam keeps its credit unless the
  fold establishes that the pinned call takes the other branch, so an exact pin
  of the owner's result from a call that provably takes the other branch no
  longer reads as strong grip (#14496). When the shape is unresolved — an
  unreadable condition, a wrapper that changes the arguments, an operand the
  parser cannot resolve - the gate keeps today's credit instead of downgrading,
  so the change only ever removes an established false promotion. The compared
  operands are resolved through their `let` initializers and value-transparent
  wrappers, a parameter rebound, destructured, mutably borrowed or assigned
  anywhere in the body (including a loop) keeps today's credit, identical arms
  stay ambiguous, and a sibling duplicate `return` after the `if` keeps the
  sibling seam's grip, not the branch-local one. The per-test record and the discriminate stage now grade the
  same oracle, and the summary names a branch refusal as one.
