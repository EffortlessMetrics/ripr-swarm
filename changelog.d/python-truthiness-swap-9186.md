<!-- section: Fixed -->
- Python: replacing a predicate with another single-operand test of the same
  parameter (`x is None` -> `not x`) no longer reads `exposed` when every
  strong related call passes an input on which the two tests agree. The
  finding is `weakly_exposed` and names a falsy non-None input as the missing
  discriminator; unreadable arguments, rebound parameters and compound
  conditions keep the existing verdict (#9186).
