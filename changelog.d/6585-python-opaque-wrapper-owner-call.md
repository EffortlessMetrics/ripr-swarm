<!-- section: Fixed -->
- Python preview no longer reads `exposed` when the only assertions call the
  changed function inside a wrapper that can ignore its result, such as
  `assert bool(parse(x)) == True` or `assert always_true(parse(x)) == True`.
  Value-preserving wrappers such as `str(...)`, `len(...)` and `round(...)`
  still credit the call (#6585).
