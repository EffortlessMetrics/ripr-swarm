<!-- section: Fixed -->
- A Python change inside a guarded arm (`if`/`elif`/`else`, an `is None` test or
  a `match` arm) of an owner whose `def` spans several lines no longer reads as
  `exposed` when no test input enters that arm. The guard walk and the
  rebinding scan both read the signature's continuation lines as body
  statements; they now start below the header (#15292).
