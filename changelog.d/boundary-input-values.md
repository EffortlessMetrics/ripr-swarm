<!-- section: Fixed -->
- A changed boundary on a value ripr cannot map to test inputs (a local
  counter such as `count`, `s.len()`, a counted local, or both sides of
  `out.len() != data.len() / 2`) now reads `infection_unknown` with "Changed
  boundary input is unresolved" instead of a missing equality discriminator
  with "observed values: unknown". A finding that read
  `propagation_unknown` (severity `note`) and now reads `infection_unknown`
  rises to severity `warning` in SARIF and GitHub annotations; one that read
  `weakly_exposed` stays `warning`
  ([#6674](https://github.com/EffortlessMetrics/ripr-swarm/issues/6674),
  [#6693](https://github.com/EffortlessMetrics/ripr-swarm/issues/6693)).
- A computed test argument such as `order_discount(base + 1)`,
  `.amount(base + 10)`, `.amount(name.len() + 10)`, `parse(s)? - 1`,
  `16usize - 1`, `1 << 4` or `&[b'f'; 16]` is no longer read as one of its
  literals. A boundary whose compared parameter receives one with a definite
  value (its variables bound to exact test values or constants), directly or
  through a helper hop such as `score(y + 1)`, stays unresolved instead of a
  missing input. Table rows and builder lines are split at top-level commas,
  and a char literal such as `','` stays one input value. Inside a row or
  setter, only a constructor (`Some(10)`, `Case { n: 1 }`, `vec![..]`) or a
  bare tuple or array passes its values through; a method or function call
  such as `.amount(x.min(10))` computes its argument. A row nested deeper
  than 32 levels is not read; it leaves the boundary unresolved only when its
  test feeds the owner a value that is not exact. When every owner call passes
  only computed arguments, a compared local such as `count` is unresolved
  rather than read from a stray test literal
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
