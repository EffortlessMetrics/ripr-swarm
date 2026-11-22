<!-- section: Added -->
- `ripr agent packet --seam-id <id> --json` now has a published schema,
  `schemas/ripr/agent-packet.schema.json`, tested against the real command's
  output for every seam of a fixture crate. The emitted bytes and
  `schema_version` `0.5` are unchanged. The `--gap-ledger` document and the
  `agent-seam-packets-json` check format are not covered yet (#3920).
