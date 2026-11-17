<!-- section: Added -->
- `ripr diff --json` now has published schemas for both documents it writes:
  `schemas/ripr/diff.schema.json` for the `ripr_diff` report and
  `schemas/ripr/diff-refusal.schema.json` for the exit-2 refusal envelope. The
  report schema states the agreement between `run_status`, `runtime_status`
  and `receipt.outcome_hint`; both share their `analysis_outcome`,
  `preview_languages`, `summary` and `run_limitations` definitions with the
  check schema, held identical by test. The emitted bytes and
  `schema_version` `0.1` are unchanged (#14067).
