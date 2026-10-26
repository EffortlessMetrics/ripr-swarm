<!-- section: Fixed -->
- `ripr check` spends less time on very large Rust test files: comment and
  string masking skips the per-byte state machine for text with no `/`, `"` or
  `'`, which is most of the short fragments the assertion readers mask. Output
  is byte-identical; a 4 MB generated test file drops about 10% cold (#10363).
