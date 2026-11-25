<!-- section: Fixed -->
- `ripr check` no longer re-parses a function and walks all of its `if`
  branches for every changed branch when it checks whether a test's call
  reaches that branch: each function's branches are read once per run. A
  changed line with many branches no longer costs its length squared there.
  Findings are unchanged (#11738).
