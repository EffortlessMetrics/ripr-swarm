<!-- section: Fixed -->
- `ripr agent start` and `ripr agent packet` now exit 3 (typed refusal) for a
  seam the inventory has but agent policy withholds, matching `agent card` and
  `agent stub`. The refusal names the policy cause (`seam_policy_omitted`
  envelope identity on `agent packet --json`) instead of "not found or hidden
  by config" or `analysis_failed` A static-limitation seam (opaque or `*_unknown`) that policy permits gets its own
  `seam_not_actionable` refusal on `agent start`, pointing at its inspection
  packet, not at policy (#10712).
