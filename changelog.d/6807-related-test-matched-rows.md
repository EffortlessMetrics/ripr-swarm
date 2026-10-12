<!-- section: Fixed -->
- Human digest: a finding's `Related test (1 of N)` line now reads
  `Related test (1 of N matched rows)` so the denominator is matched
  related-test rows (one assertion, plus examined-miss rows), not distinct
  tests. A packed finding with one test and 81 `assert_eq!` rows no longer
  reads as 81 tests. JSON `related_tests_total` is unchanged
  ([#6807](https://github.com/EffortlessMetrics/ripr-swarm/issues/6807)).
