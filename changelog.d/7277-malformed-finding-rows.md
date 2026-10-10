<!-- section: Fixed -->
- DX scoreboard trust no longer scores a clean
  `trust.self_contradictions` zero from a findings row that is missing a
  string `id` / `classification`, a numeric `related_tests_total`, or
  (when R3 can apply) an `evidence` array. Rendering prefixes and a
  missing findings array stay incomplete; well-typed R2/R3 rows still
  increment ([#7277](https://github.com/EffortlessMetrics/ripr-swarm/issues/7277)).
