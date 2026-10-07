<!-- section: Fixed -->
- `ripr plus`: when a run fails to read or compose its artifact, it now keeps
  the receipt an earlier `ripr plus` run composed as
  `ripr-plus.last-good.{json,md}`. Before this fix, only a `pass` or `warn`
  receipt from another producer was kept, because every receipt `ripr plus`
  composes is `indeterminate` with cause `quality_evidence_incomplete`. An
  earlier failed run's error receipt is still never kept, and the kept copy is
  still not current evidence
  ([#6295](https://github.com/EffortlessMetrics/ripr-swarm/issues/6295)).
