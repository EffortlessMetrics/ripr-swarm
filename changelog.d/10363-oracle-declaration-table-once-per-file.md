<!-- section: Fixed -->
- `ripr check` no longer slows down quadratically on a very large Rust test
  file: the declaration table that grades assertions is built once per file
  instead of being re-compared against the whole file for every assertion. A
  4 MB generated test file drops from about 35 s to 11 s cold (#10363).
