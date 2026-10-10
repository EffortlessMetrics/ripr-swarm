<!-- section: Fixed -->
- The DX scoreboard now records both `speed.warm_check_*` samples as
  `Failed` when warm-up capture succeeds and the measured capture returns
  an instrument error. `--gate` fails that run even without a baseline.
  Timeout and a skipped second run remain `Incomplete`
  ([#7275](https://github.com/EffortlessMetrics/ripr-swarm/issues/7275)).
