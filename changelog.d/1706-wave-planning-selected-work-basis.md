<!-- section: Added -->
- `cargo xtask work wave check` validates committed wave plans against the
  compiled work-portfolio basis: every plan consumes the complete selected
  candidate set and partitions it into waves over explicit resources
  (`campaign-record:<id>`, `portfolio-schema`, `spec-ledger`,
  `output-contract:<id>`, `issue:<n>`, `pr:<n>`, `worktree:<path>`); the
  retired generic `active-goal` resource fails closed, shared exclusive
  resources and over-capacity waves fail with exact reconcile routes, and
  disjoint cross-campaign waves are admitted (#1706).
