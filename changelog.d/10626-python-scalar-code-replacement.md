<!-- section: Fixed -->
- Python: a scalar module-constant edit is no longer credited when any workspace
  file assigns a function's `__code__`, `__defaults__`, `__kwdefaults__` or
  `__closure__`, which can replace the reader without rebinding its name
  (#10626).
