<!-- section: Internal / CI -->
- `cargo xtask check-evidence-promotion-honesty` now judges each seam's
  `evidence_record.evidence_path` rows - stage, state, confidence and summary -
  against the regeneration, not just the flat `evidence` map, so a producer
  wording change can no longer leave a stale summary passing green. The
  comparison is scoped to captures recorded at the analyzer's current
  `CACHE_SCHEMA_VERSION`; a capture a pin holds at an older generation stays
  tolerated, and 28 of the 29 regenerable reports stay unjudged until the next
  corpus recapture (#14555).
