<!-- section: Fixed -->
- Doctor: `ripr doctor` no longer reports "checks passed" silently when the
  cache directory cannot be written (every run would recompute). It now
  prints `! Cache not writable: <cause>` and suggests pointing
  `RIPR_CACHE_DIR` at a writable directory. The probe never creates the
  cache directory, it also checks layer directories that already exist,
  and the warning does not fail doctor. `ripr doctor --json` does not run
  this probe yet
  ([#5297](https://github.com/EffortlessMetrics/ripr-swarm/pull/5297)).
