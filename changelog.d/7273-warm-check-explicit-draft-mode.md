<!-- section: Fixed -->
- The DX scoreboard now passes `--mode draft` on the measured warm-check
  command, matching the admission subject by construction. A checkout
  `ripr.toml` that sets a non-draft analysis mode can no longer retarget
  the producer and have an otherwise complete sample rejected as the
  wrong subject
  ([#7273](https://github.com/EffortlessMetrics/ripr-swarm/issues/7273)).
