<!-- section: Added -->
- `ripr: Start Current Repair` runs the shared `ripr agent repair --phase before`
  flow when the LSP exposes a validated seam-bound repair handoff, shows the
  bounded edit-cage summary (including every `must_not_change` rule), and
  idempotently resumes only inventory rows whose disposition is `resumable`
  (#4645).
