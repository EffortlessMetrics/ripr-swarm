<!-- section: Fixed -->
- Python preview no longer reports `exposed` for an edit that only switches
  `strip`/`lstrip`/`rstrip` on a parameter, or changes the digit count of
  `round(<parameter>, n)`, when no related test call passes an input the two
  versions treat differently; it reads `weakly_exposed` and names that input
  (#16203).
