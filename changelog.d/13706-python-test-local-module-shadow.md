<!-- section: Fixed -->
- Python module identity: a test-local helper module (`tests/core.py`) that pytest's prepend import mode puts ahead on `sys.path` for a test now shadows a same-named project module, so `from core import limit` is no longer credited to the project's `core.py` (#13706).
