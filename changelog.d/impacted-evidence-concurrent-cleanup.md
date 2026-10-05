<!-- section: Fixed -->
- `ripr impacted-evidence`: a run that refuses bad `--pr-evidence` no longer
  deletes `latest.{json,md}` that a concurrent run wrote after it started. It
  removes only outputs that already existed, unchanged, when it began, and
  says which newer outputs it left in place. An output whose metadata it
  cannot read is reported as a cleanup failure instead of being skipped as
  absent
  ([#5307](https://github.com/EffortlessMetrics/ripr-swarm/issues/5307)).
