<!-- section: Changed -->
- Python: an edit to a one-line module-level dict constant is credited when a
  test's assertion calls a module function that reads the table with
  `.get(key)` or `[key]` and the assertion differs between the old and new
  dict. Any mutation, alias or reflective access to the table elsewhere in the
  workspace withholds the credit. `py-shapes-package-export-lookup` now reads
  `credited`; the Python corpus stays at 0 false verdicts in 311 cases (#9619).
