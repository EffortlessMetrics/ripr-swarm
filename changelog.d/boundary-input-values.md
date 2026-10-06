<!-- section: Fixed -->
- A changed boundary on a value ripr cannot map to test inputs (a local
  counter such as `count`, `s.len()`, a counted local, or both sides of
  `out.len() != data.len() / 2`) now reads `infection_unknown` with "Changed
  boundary input is unresolved" instead of a missing equality discriminator
  with "observed values: unknown"
  ([#6674](https://github.com/EffortlessMetrics/ripr-swarm/issues/6674),
  [#6693](https://github.com/EffortlessMetrics/ripr-swarm/issues/6693)).
- A computed test argument such as `order_discount(base + 1)` or
  `&[b'f'; 16]` is no longer read as one of its literals; a boundary whose
  compared parameter receives one with a definite value (its variables bound
  to exact test values or constants), directly or through a helper hop such
  as `score(y + 1)`, stays unresolved instead of a missing input
  ([#6672](https://github.com/EffortlessMetrics/ripr-swarm/issues/6672)).
- A computed boundary operand such as `CURRENT - 2` or `2 + 2`, including one
  inside an `&&`/`||` condition, is no longer read as the literal it contains.
  A same-file `CONST ± N` resolves to its value when the constant is a plain
  decimal literal and otherwise stays unresolved, so ripr no longer names a
  wrong boundary value in the repair hint
  ([#6671](https://github.com/EffortlessMetrics/ripr-swarm/issues/6671)).
- The human report's "Why unknown" line for `infection_unknown` says the
  change "reaches a sink" only when the propagation stage is `yes`; otherwise
  it says no sink the change reaches was established.
