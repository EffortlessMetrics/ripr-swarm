<!-- section: Features -->
- One canonical next action (#6304, RIPR-SPEC-0242): a single selector
  projects the primary action for check triage, RepairCard `next_action`,
  and RepairAttempt status from a closed eight-class vocabulary with exact
  subject/currentness binding and typed stops. Card prose and card/status
  JSON project the same DTO; `next_action_class` values are now governed
  output-contract enums.
