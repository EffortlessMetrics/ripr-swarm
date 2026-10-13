<!-- section: Added -->
- Verdict corpus: oracle-strength accuracy is now measured apart from the
  line verdict. A case may label the related-test oracle kind and strength
  a spec adopts (`expected_oracle`); the report's new `oracle_accuracy`
  section scores ripr's strongest reported entry as match, overclaim,
  underclaim, kind_mismatch or unobserved. All 28 RIPR-SPEC-0231 cases and
  six RIPR-SPEC-0227 `is_err()`/`unwrap_err()` cases are labeled: 26/34
  match, 5/34 overclaim (RIPR-SPEC-0231 rule 1 inequalities) and 3/34
  underclaim (ripr relates the test but reads no oracle) today. `cargo xtask verdict-corpus check` fails
  when matches fall below or overclaims rise above the `[oracle]` floor and
  ceiling in `spec-coverage.toml`, and the dx-scoreboard trust board tracks
  `trust.verdict_corpus_oracle_accuracy` (target 1.0) and
  `trust.verdict_corpus_oracle_overclaim_rate` (target 0)
  ([#6640](https://github.com/EffortlessMetrics/ripr-swarm/issues/6640)).
