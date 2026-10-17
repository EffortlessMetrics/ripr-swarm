<!-- section: Added -->
- `cargo xtask work result check` validates committed task-result rows
  against the compiled work-portfolio basis: every result must bind the
  repository, portfolio-snapshot identity, basis head and the selected-work
  identity it extends, reusing the RIPR-SPEC-0244 selection law engine;
  wrong repository, snapshot, issue/action, basis, worktree or head fails
  stale/wrong-input with exact recompile/reconcile routes, evidence-free
  results fail as claims, and legacy goal/work-item references resolve
  read-only with no authority (#1706).
