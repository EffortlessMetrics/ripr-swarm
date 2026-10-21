<!-- section: Fixed -->
- `cargo xtask mutation-spot-check` now classifies mutation-calibration
  reports at schema 0.2 and 0.3 (0.3 adds provenance and how_to_read without
  changing the joined-record shape), and a report at any other schema version
  is refused with an actionable message naming the found version, the
  supported versions, and the renderer's owning
  `MUTATION_CALIBRATION_SCHEMA_VERSION` constant (#8996).
