<!-- section: Fixed -->
- Repairs targeting external Rust test files now require pure insertion of new
  test functions and preserve existing tests byte-for-byte; existing selected targets
  must be committed before the repair starts, while cleanly absent targets may
  be created by adding new test functions (#8710).
