<!-- section: Fixed -->
- `ripr check` no longer rescans a whole test file for renaming and
  standard-library glob imports once per test in it. The scan runs once per
  test file and each test's question is a lookup, so a file holding
  thousands of tests no longer costs time that grows with the square of its
  size there. Findings are unchanged (#11738).
