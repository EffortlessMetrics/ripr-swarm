<!-- section: Added -->
- `ripr agent packet --gap-ledger <path> --gap-id <id> --json` now has a
  published schema, `schemas/ripr/agent-gap-packet.schema.json`, covering the
  advisory packet and the blocked, review-only candidate. It is tested against
  the real command's output from a generated gap ledger. The emitted bytes and
  `schema_version` `0.5` are unchanged (#3920).
