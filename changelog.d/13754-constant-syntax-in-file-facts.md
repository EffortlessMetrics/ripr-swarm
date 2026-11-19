<!-- section: Changed -->
- `ripr check` and editor saves no longer re-parse every workspace file to
  build the table of `const` and `static` items. The parser that already reads
  each file records where those items are, and the table reads them from
  there, with identical findings (#13754).
