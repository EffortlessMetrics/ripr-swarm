<!-- section: Fixed -->
- `ripr check` no longer reports a gap for a copy edit of string-literal text
  outside every function, such as a row of a `static` help table. With only
  same-file or name-token tests related, the finding read `weakly_exposed`
  beside an unrelated test; it now reads `static_unknown` under the new
  `rust_unowned_literal_reader_unresolved` limitation and asks whether a test
  compares the text. Nothing is credited, and a test that quotes the literal
  whole keeps its own reading (#13843).
