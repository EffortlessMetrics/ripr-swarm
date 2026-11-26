<!-- section: Fixed -->
- Python module identity: a helper module that a test file pytest collects in
  a parent directory puts on `sys.path` now shadows a same-named project module
  for tests in the subdirectories below it, as a full pytest run does. A
  `from core import price` in `tests/unit/test_x.py` is no longer credited to
  the project's `core.py` when `tests/test_a.py` is collected beside a helper
  `tests/core.py`. Helper modules pytest does not collect (`tests/helpers.py`)
  still add nothing (#14083).
