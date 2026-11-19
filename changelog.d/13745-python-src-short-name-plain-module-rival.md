<!-- section: Fixed -->
- Python: a src-layout short module name (`svc/a/src/core.py` → `core`) is no longer credited to a test's `from core import ...` when a plain module of the same name is importable from the same project (a repo-root `core.py`, or `svc/a/core.py` beside `svc/a/src/`). Which module loads depends on `sys.path` order, so the name identifies neither (#13745).
