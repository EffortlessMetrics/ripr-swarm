<!-- section: Fixed -->
- `ripr calibrate cargo-mutants` now fills the Mutation operator column from
  cargo-mutants `genre` (`FnValue`, `BinaryOperator`, …) instead of the
  `replacement` expression, so common mutants no longer render as bare
  `None` / `()` (#7743). Records that already supply `operator` are unchanged.
