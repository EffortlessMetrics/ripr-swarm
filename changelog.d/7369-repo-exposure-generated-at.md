<!-- section: Added -->
- `repo-exposure-json` and `repo-seams-json` artifacts now carry a
  top-level `generated_at` stamp (`unix_ms:<millis>`, the same clock
  shape the gap-decision ledger uses) in their producer identity
  envelope. A retained CI artifact can now be aged from its own bytes
  instead of reverse-engineering freshness by comparing
  `artifact.repository.head` against the live repo, and the badge
  layer's staleness machinery gains the observation-time anchor its
  source artifact previously lacked. The timestamp is stamped once per
  artifact so the two-pass content-hash scheme keeps it fixed across
  both passes, and it is optional at read time so artifacts produced
  before the field existed keep validating (#7369).
