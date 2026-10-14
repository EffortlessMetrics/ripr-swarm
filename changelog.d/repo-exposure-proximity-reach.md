<!-- section: Fixed -->
- Repo exposure: a seam no longer reads `reach: yes`, or
  `strongly_gripped`, from related tests that only share its owner's file or
  module, or that only assert a token the seam names. Repo mode now uses the
  diff-mode reach rule: when every related test is such a relation, reach is
  `no` (the seam reads `ungripped`, no static path) unless something could
  still run the owner unseen (a doctest or caller names it, a trait impl, a
  test macro, or an unresolved transitive path), which keeps reach `weak`.
  On bytesize `66a3715`, the uncalled `as_kb`, `as_mib`, `as_gib`, `as_tb`,
  `as_pb` and `as_eb` read `ungripped` instead of `activation_unknown` with
  9 phantom exact-value tests. The called siblings still read
  `strongly_gripped`
  ([#5335](https://github.com/EffortlessMetrics/ripr-swarm/issues/5335)).
