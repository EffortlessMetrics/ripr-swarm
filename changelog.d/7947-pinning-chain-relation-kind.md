<!-- section: Fixed -->
- An `assert_eq!(chain, true)` whose `&&` chain has a pinning `==` conjunct
  no longer takes its oracle kind from a negated or `!=` conjunct:
  `assert_eq!(a == b && r != Err(E::Bad), true)` and
  `assert_eq!(a == b && !matches!(r, Err(E::Bad)), true)` now read
  `exact_value` / strong from `a == b` instead of `exact_error_variant`.
  The relation-only conjunct pins nothing and lends no kind; the reading
  never rises above the whole assertion's (RIPR-SPEC-0231 rule 1,
  example 33;
  [#7947](https://github.com/EffortlessMetrics/ripr-swarm/issues/7947)).
