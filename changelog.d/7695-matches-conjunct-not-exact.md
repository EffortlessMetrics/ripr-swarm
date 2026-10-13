<!-- section: Fixed -->
- A `matches!` inside an `assert!`, `debug_assert!`, `prop_assert!` or
  `ensure!` `&&` chain no longer earns `exact_value` / strong
  whatever its pattern: `assert!(ready && matches!(r, Ok(_)))` now reads
  `relational_check` / weak, no stronger than its strongest conjunct read
  alone. A chain with a pinning `==` conjunct (`total == 4 && ..`) keeps its
  reading (RIPR-SPEC-0231 rule 3, example 7;
  [#7695](https://github.com/EffortlessMetrics/ripr-swarm/issues/7695)).
