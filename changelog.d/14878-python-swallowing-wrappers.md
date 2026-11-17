<!-- section: Changed -->
- Python: a test whose only check is `wrapper(lambda: helper(...))`, where
  the test module's `wrapper` provably swallows the `AssertionError` its
  assert-only `helper` raises (an `except Exception: return None`, a
  partial or `isinstance`-guarded re-raise, or a `finally: return`), is now
  read as holding no assertion. The changed behavior reports a gap instead
  of an unreadable custom assertion helper. A re-raising wrapper keeps its
  credit, and any wrapper or helper shape RIPR cannot bound stays limited
  (#13723).
