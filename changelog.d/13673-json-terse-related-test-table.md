<!-- section: Changed -->
- `ripr check --format json-terse` emits each repeated related-test row core
  (name, file, line, oracle, oracle kind and strength) once in a
  document-level `related_test_table`. Rows reference it by `table_row` and
  keep their per-finding relation fields. On the five-gap budget fixture the
  terse document drops from 40,612 to 36,684 bytes, and its ceiling is lowered
  from 42,500 to 38,500 (#13673).
