<!-- section: Fixed -->
- A Python change to one arm of a conditional expression (`a if c else b`) no
  longer reads `exposed` when every strong related test call provably leaves
  that arm unevaluated, for example `loud` is omitted and defaults to `False`
  while the `loud` arm changed. The finding reports `weakly_exposed` naming the
  condition a call would need to satisfy. A changed condition, a rebound
  parameter, nested expressions, comments and unreadable call arguments keep
  the existing verdict (#9184).
