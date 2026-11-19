<!-- section: Internal / CI -->
- `cargo xtask goldens check-with-fixtures` prints one explicit pass line with
  the compared-fixture count on a clean run, so a real pass and a no-op no
  longer look the same (#15153).
