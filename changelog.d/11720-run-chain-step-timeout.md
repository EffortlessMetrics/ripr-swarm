<!-- section: Added -->
- `ripr first-pr --run-chain` steps are bounded: 2 hours by default, set
  `RIPR_FIRST_PR_RUN_CHAIN_STEP_TIMEOUT_SECS` (whole seconds, `0` for no bound).
  A step past its bound is terminated and named as `timed out after <N>
  second(s)`, its newly created files are removed, and the run exits 2 with no
  packet. A value that is not a whole number is now refused instead of silently
  meaning unbounded, and a grandchild holding the step's pipe no longer extends
  the chain past its bound (#11720).
