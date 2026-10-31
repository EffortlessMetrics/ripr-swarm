<!-- section: Added -->
- `ripr first-pr --run-chain` now bounds each step: 30 minutes by default,
  overridable with `RIPR_RUN_CHAIN_STEP_TIMEOUT` (whole seconds). A step past
  its bound is terminated and reported as `timed out after <N>`, its newly created
  files are removed, and the run exits 2 with no packet (#11720).
