<!-- section: Fixed -->
- `ripr impacted-evidence`: a run that refuses bad `--pr-evidence` no longer
  deletes `latest.{json,md}` that a concurrent run wrote after it started. It
  removes only outputs that already existed, unchanged, when it began, and
  says which newer outputs it left in place
  ([#5307](https://github.com/EffortlessMetrics/ripr-swarm/issues/5307)).
