<!-- section: Fixed -->
- Rust return-value and field findings no longer read `exposed` from an
  assertion that only shares a word with the changed line. A token naming
  the owner's own parameter or `let` local, or a numeric literal, now
  confirms observation only in an assertion that calls the owner, so a test
  of `subtotal(3, 100)` no longer covers a changed `subtotal * 8 / 100` in
  `tax`. An `assert_eq!` that computes its expected value through the
  changed function (`assert_eq!(invoice(3, 100), sub + tax(sub))`, where
  `invoice` calls `tax`) is now a weak, unconfirmed oracle
  ([#5830](https://github.com/EffortlessMetrics/ripr-swarm/issues/5830),
  [#6970](https://github.com/EffortlessMetrics/ripr-swarm/pull/6970)).
