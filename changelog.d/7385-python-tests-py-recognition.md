<!-- section: Fixed -->
- The Python preview adapter now recognizes the Django `startapp`
  conventions `tests.py` and `test.py` (at any package level, including
  the repository root) as test files, so exact-value assertions in those
  files credit the owners they observe instead of reporting
  `no_static_path` with advice to add a test that already exists. The
  unlinked-Python-tests note for Rust `no_static_path` findings counts
  the same module names (#7385).
