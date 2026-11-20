<!-- section: Added -->
- The refusal envelopes of `ripr rerun --json`, `ripr agent brief --json` and
  `ripr agent packet --json` now have published schemas:
  `schemas/ripr/rerun-refusal.schema.json`,
  `schemas/ripr/agent-brief-refusal.schema.json` and
  `schemas/ripr/agent-packet-refusal.schema.json`. Each is the diff refusal
  schema with the command's own version and scope, held to that difference by
  test, and tested against the real command's output. The emitted bytes are
  unchanged (#3920).
