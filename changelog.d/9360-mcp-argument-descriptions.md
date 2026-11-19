<!-- section: Fixed -->
- `ripr mcp`: tool arguments say what to pass. `ripr_refresh` `base` names the
  failure an agent actually sees (`analysis_failed`, not the internal
  `base_unresolvable`), `ripr_list_gaps` `class` advertises the seven exposure
  classes it accepts, and `snapshot_id`, `canonical_id`, `attempt_id`,
  `receipt_id` and the attempt-list `offset` and `limit` each carry one short
  line where they had none (#9360).
