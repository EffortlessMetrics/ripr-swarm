<!-- section: Docs -->
- The `Context Packet` contract in `docs/OUTPUT_SCHEMA.md` now names the
  top-level `canonical_gap_id` member that `ripr context --json` emits
  whenever the finding carries a canonical gap: the example shows it and the
  prose states it is conditionally present and shares its value with
  `finding_alignment` and `ripr receipt write --gap` (#7750).
