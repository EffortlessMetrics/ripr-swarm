<!-- section: Fixed -->
- `ripr calibrate cargo-mutants --format md` shows imported mutant ids,
  operators, test commands and paths as literal text. A generic such as
  `Option<Vec<String>>` keeps its type arguments instead of parsing as raw
  HTML, an id containing a backtick stays one code span, and `@name` in a
  mutant description no longer notifies an account when the report is posted
  (#14092).
