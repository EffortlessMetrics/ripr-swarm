<!-- section: Internal / CI -->
- `cargo policy check-doc-roles` runs the existing proposal and ADR section
  checks without the RIPR product dependency; the xtask facade retains the same
  checker, report and failure behavior. Hosted docs-gate routing is unchanged (#14644).
