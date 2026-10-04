<!-- section: Fixed -->
- Pilot: `ripr pilot` no longer fills its top recommendations with adjacent
  seams of one function. Within each grip class, every function gets one
  pick before any gets a second, and `pilot-summary.md` says how many more
  actionable seams a listed function has. On the five mutation spot-check
  crates the top ten went from 19 to 50 distinct functions out of 50, and
  picks refuted by cargo-mutants fell from 29 to 22 (precision 29.3% to
  37.1%)
  ([#5770](https://github.com/EffortlessMetrics/ripr-swarm/issues/5770)).
