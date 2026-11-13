<!-- section: Fixed -->
- Repo seam grip: a branch-local `return_value` seam is credited only by an
  oracle whose call statically reaches that branch, so an exact pin of the
  owner's result from a call that takes the other branch no longer reads as
  strong grip (#14496). The compared operands are resolved through their `let`
  initializers and value-transparent wrappers, every enclosing `if` on the
  path must be taken, a parameter rebound, mutated or bound by a `for`
  pattern anywhere in the body (including a loop) keeps today's credit,
  identical arms stay ambiguous, and
  a sibling duplicate `return` after the `if` keeps the call's grip. The
  per-test record now agrees with the discriminate stage.
