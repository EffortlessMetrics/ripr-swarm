<!-- section: Internal / CI -->
- Cargo intermediates now use worktree-keyed trees under a bounded repo-scoped
  Cargo-home root, with dry-run cleanup receipts that preserve `target/ripr`
  evidence and refuse shared or ambiguous deletion paths (#13965).
