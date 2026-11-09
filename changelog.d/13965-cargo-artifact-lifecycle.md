<!-- section: Internal / CI -->
- Cargo intermediates now use manifest-keyed trees in a bounded same-volume
  sibling-checkout pool, with dry-run cleanup receipts that preserve
  `target/ripr`, protect live worktrees, and refuse ambiguous deletion paths
  (#13965).
