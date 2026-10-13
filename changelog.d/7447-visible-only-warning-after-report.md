<!-- section: Fixed -->
- `ripr gate evaluate` prints the default visible-only stderr warning only
  after a report builds for a completed evaluation. Missing-input usage
  errors and `config_error` runs no longer lead with advice about a mode
  the run never used (#7447). RIPR-SPEC-0014 and `ripr gate --help` now
  name that completed-evaluation bound.
