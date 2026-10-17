<!-- section: Fixed -->
- Rust: a guard-only match-arm edit (`n if 0 > n =>` from `n if n < 0 =>`)
  now reads as the guard's predicate finding alone. The arm head no longer
  adds an unconfirmed `weakly_exposed` finding, and the old line is not
  probed again as deleted behavior. A bare guard binding in the owner's only
  `match` on a parameter reads that parameter's test inputs, so a guard
  boundary no test input reaches is named (`p == 100`)
  ([#7811](https://github.com/EffortlessMetrics/ripr-swarm/pull/7811)).
