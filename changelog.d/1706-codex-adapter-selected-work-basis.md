<!-- section: Added -->
- `cargo xtask work adapter check` validates committed Codex delegation
  rows against the compiled work-portfolio basis: every delegation binds
  the repository, portfolio-snapshot identity, basis head and the full
  selected-work identity it delegates, one closed project role and one
  closed flow stage, the subagent consumes the selected-work context
  packet plus relevant overflow and never re-reads the portfolio,
  write-capable builder delegations require a prepared claim or isolated
  worktree, subagents never claim, and a wrong repository, snapshot, basis
  or legacy authority attempt fails with exact recompile/reconcile routes
  (#1706).
