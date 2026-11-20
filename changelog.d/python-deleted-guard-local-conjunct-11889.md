<!-- section: Fixed -->
- Python: deleting a guard block such as `if seq_index is None and start < 0:`
  no longer reads `exposed` when no test input satisfies the parameter part of
  the guard. A guard that also named a local was not read at all before, so a
  block no test entered counted as noticed by tests (#11889).
