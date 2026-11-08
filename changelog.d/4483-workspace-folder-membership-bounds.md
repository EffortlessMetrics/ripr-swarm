<!-- section: Fixed -->
- LSP: workspace-folder membership and root-disclosure strings are now capped at
  reviewed limits (entry count, per-URI/path bytes, aggregate bytes, and
  bounded list summaries). Over-limit initialize lists, deltas and
  reconciliation replacements are rejected with `membership_capacity_exceeded`
  before mutating stored folder state (#4483).
