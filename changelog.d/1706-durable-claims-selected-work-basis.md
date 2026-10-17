<!-- section: Added -->
- `cargo xtask work claim check` validates committed durable-claim rows
  against the compiled work-portfolio basis: every claim binds one selected
  issue/action and the exact basis, a stale basis keeps the claim recorded
  for reconcile (never release or takeover), one worktree cannot serve two
  selections and one selection cannot be double-claimed, and legacy
  goal/work-item references resolve read-only with no authority (#1706).
