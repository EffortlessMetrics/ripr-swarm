<!-- section: Added -->
- `ripr agent status --json` now has a published schema,
  `schemas/ripr/agent-status.schema.json`, tested against the real command's
  output at four stages of a repair loop (fresh, awaiting the test edit,
  finished, failed). The emitted bytes and `schema_version` `0.1` are
  unchanged (#3920).
