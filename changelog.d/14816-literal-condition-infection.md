<!-- section: Fixed -->
- Rust: an `if` condition forced to a literal (`if x > 0` → `if true`,
  Stryker's ConditionalExpression) no longer reads `infection_unknown` for
  every test. When the diff changes only that condition in the owner, ripr
  folds the old and new owner at the related tests' literal calls: a pin or a
  bare call at an input that took the other branch on the base side credits
  infection (an exact pin now reads `weakly_exposed`, since the predicate's
  reveal pairing still reads the check as unconfirmed), and pins only at
  inputs that already took the forced branch read a weak infection. An
  argument or body ripr cannot fold, a line changed beyond its condition, or
  an owner whose base side is not verified as the same function keeps
  `infection_unknown` (#14816).
