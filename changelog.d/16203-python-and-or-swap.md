<!-- section: Fixed -->
- Python preview no longer reports `exposed` for a `return`/`if`/`elif`
  condition whose only edit swaps `and` for `or` when every related test call
  makes both operands agree; it reads `weakly_exposed` and names a call where
  the operands differ (#16203).
