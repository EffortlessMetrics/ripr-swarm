<!-- section: Fixed -->
- A Python `conftest.py` whose `collect_ignore` is guarded by an `if` (for
  example a Python-version check) now withholds credit only from the files
  either branch could ignore, instead of marking every test in the directory
  unevaluated, and the static limit now names the conftest `collect_ignore`
  rather than blaming a test decorator (#15631).
