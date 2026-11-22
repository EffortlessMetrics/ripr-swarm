<!-- section: Added -->
- `ripr check --format agent-seam-packets-json` now has a published schema,
  `schemas/ripr/agent-seam-packets.schema.json`, which shares its packet
  definitions with `agent-packet.schema.json` and is tested against the real
  command's output, including its refusal shape. The emitted bytes and
  `schema_version` `0.5` are unchanged (#3920).
