<!-- section: Fixed -->
- Python: a property check no longer pins a value. An `==` assertion that
  compares the owner with itself (`f(f(x)) == f(x)`) or with a loop variable it
  was passed (`for n in range(1, 5): assert f(1, n) == n`) reads as a weak
  relational check instead of `exposed`, so the two property-loop trap rows
  (`py-grid-returns-property`, `py-grid-arithmetic-property`) read `gap`
  (#9594).
