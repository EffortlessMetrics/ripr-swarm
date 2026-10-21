<!-- section: Fixed -->
- Python: unbalanced brackets inside string literals (such as `"\x1b[31m"` in a
  large parametrize table) no longer accumulate in the parse nesting budget, so
  test files with bracket-bearing literals are analyzed instead of refused as
  `language_scope_unsupported` (#8561).
