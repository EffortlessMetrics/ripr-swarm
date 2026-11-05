<!-- section: Added -->
- Rust analysis recognizes a same-file test helper whose builtin `u32` equality
  is asserted directly over a literal input table, with owner, input and
  assertion identity retained; discarded, lossy and shadowed helpers remain
  limited (#13181).
