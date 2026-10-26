<!-- section: Fixed -->
- DX scoreboard: the `trust.self_contradictions` rule R1 no longer counts a
  `reach: no` seam that lists only proximity-related tests (`same_test_file`,
  `same_module`, `assertion_target_affinity` on a function owner) as
  self-contradictory. Repo-exposure reach has kept those tests listed as
  suggestions beside `reach: no` since #6866; R1 now asks the analyzer's own
  `RelationReason::establishes_reach` rule, so ripgrep reads 0 again instead of
  4 (#10809).
