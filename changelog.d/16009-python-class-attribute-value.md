<!-- section: Fixed -->
- Python: changing a non-string class attribute (for example
  `_PER_YEAR = 12` to `13`) no longer reads `exposed` through tests that build
  the class and assert on a method that never reads it. It reads
  `weakly_exposed` unless a related test reads the attribute or calls a method
  of the class that reads it (#16009).
