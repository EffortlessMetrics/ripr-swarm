<!-- section: Fixed -->
- `ripr check` no longer re-reads a whole test module for every test in it
  when deciding whether each test's assertions run. Whether the module
  rebinds `#[test]` and which gating attributes its enclosing items carry
  are now read once per module, so a file holding thousands of tests no
  longer costs time that grows with the square of its size there. Findings
  are unchanged (#11738).
