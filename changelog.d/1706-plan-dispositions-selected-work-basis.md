<!-- section: Added -->
- `cargo xtask work plan check` validates committed plan rows against the
  compiled work-portfolio basis: every plan binds the repository,
  portfolio-snapshot identity, basis head and the full selected-work
  identity it plans for, planning emits one closed disposition and is
  read-only so a row that would replace or reorder the whole portfolio
  fails visibly, campaign-targeting dispositions must name a recorded
  campaign, a blocked disposition must name its blocker, a plan derived
  from nothing is a claim, and a wrong repository, snapshot, basis or
  legacy authority attempt fails with exact recompile/reconcile routes
  (#1706).
