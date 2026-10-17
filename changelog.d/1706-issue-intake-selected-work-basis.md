<!-- section: Added -->
- `cargo xtask work intake check` validates committed issue-intake rows
  against the compiled work-portfolio basis: every intake binds the
  repository, portfolio-snapshot identity, basis head, current main and
  one issue the captured portfolio records; campaign placement and
  relevant campaigns must be recorded campaigns, but an out-of-campaign
  issue may still qualify because placement stays a later root/planning
  decision, open overlapping PRs and claims stay visible before any
  mutation, and an empty contract set, an unknown issue, a wrong
  repository or a legacy authority attempt fails with exact
  recompile/reconcile routes (#1706).
