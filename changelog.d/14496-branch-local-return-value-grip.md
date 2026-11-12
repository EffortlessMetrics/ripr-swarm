<!-- section: Fixed -->
- Repo seam grip: a branch-local `return_value` seam is credited only by an
  oracle whose call statically reaches that branch, so an exact pin of the
  owner's result from a call that takes the other branch no longer reads as
  strong grip (#14496).
