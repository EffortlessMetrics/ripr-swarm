<!-- section: Added -->
- `ripr diff --json` now has a published schema for its `ripr_diff` report,
  `schemas/ripr/diff.schema.json`. It states the agreement between
  `run_status`, `runtime_status` and `receipt.outcome_hint`, and shares its
  `analysis_outcome` and `preview_languages` definitions with the check
  schema, held identical by test. The emitted bytes and `schema_version`
  `0.1` are unchanged; the refusal envelope the command writes when it cannot
  produce a report is a separate document the schema does not describe (#14067).
