<!-- section: Fixed -->
- Python module identity: a test module that pytest collects in a parent
  directory of the same project now puts that directory on `sys.path` for the
  tests below it, as a pytest run that collects the parent does. A helper
  module there shadows a same-named project module, so a
  `from core import price` in `tests/unit/test_x.py` is no longer credited to
  the project's `core.py` when `tests/test_a.py` is collected beside a helper
  `tests/core.py`. Helper modules pytest does not collect (`tests/helpers.py`)
  and test modules above the test's project root add nothing (#14083).
