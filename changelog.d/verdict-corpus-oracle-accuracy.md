<!-- section: Internal / CI -->
- Verdict corpus: oracle-strength accuracy is now measured apart from the
  line verdict. A case may label the related-test oracle kind and strength
  a spec adopts (`expected_oracle`); the report's new `oracle_accuracy`
  section scores ripr's strongest reported entry as match, overclaim,
  underclaim, kind_mismatch or unobserved. All 32 RIPR-SPEC-0231 cases and
  six RIPR-SPEC-0227 `is_err()`/`unwrap_err()` cases are labeled: 35/38
  match, 0/38 overclaim and 3/38 underclaim (ripr relates the test but
  reads no oracle) today; the five RIPR-SPEC-0231 rule 1 inequality
  overclaims were fixed by #6670 and reblessed in #7631, the `ensure!`
  disjunction overclaim by #7558, the `matches!` conjunct overclaim by
  #7695, the nested `matches!` overclaim by #7715, and the bool-compared
  inequality overclaim by #7598. `cargo xtask verdict-corpus check` fails
  when matches fall below or overclaims rise above the `[oracle]` floor and
  ceiling in `spec-coverage.toml`, and the dx-scoreboard trust board tracks
  `trust.verdict_corpus_oracle_accuracy` (target 1.0) and
  `trust.verdict_corpus_oracle_overclaim_rate` (target 0)
  ([#6640](https://github.com/EffortlessMetrics/ripr-swarm/issues/6640)).
