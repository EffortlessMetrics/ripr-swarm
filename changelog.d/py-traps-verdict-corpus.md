<!-- section: Internal / CI -->
- The Python verdict corpus gains the `authored-py-trap-kit` subject: 30
  cases covering 17 false-credit traps (dead `if False:` branch, skipped module,
  swallowed assertion, empty table and others), each with a control that notices
  the change, and fills the Python `[grid]` ledger table. Six rows are false
  and listed under #9691; `py-trap-star-imported` now credits correctly and is
  re-blessed with its label (#9691).
