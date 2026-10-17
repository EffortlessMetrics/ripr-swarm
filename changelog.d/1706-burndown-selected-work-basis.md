<!-- section: Added -->
- `cargo xtask work burndown check` validates committed burn-down rows
  against the compiled work-portfolio basis: every burn-down binds the
  repository, portfolio-snapshot identity, basis head and the
  selected-work identity it tracks, reusing the RIPR-SPEC-0244 selection
  law engine; completion derives from issue/spec acceptance,
  implementation slices, PRs and exact-head review/proof evidence rows,
  campaign and work-item records are plan context only and a `complete`
  burn-down over an open or stale row fails with the reconcile route, and
  legacy goal/work-item refs resolve read-only with no authority (#1706).
