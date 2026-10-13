<!-- section: Fixed -->
- A `matches!` inside an `assert!`, `debug_assert!`, `prop_assert!` or
  `ensure!` `&&` chain no longer earns `exact_value` / strong
  whatever its pattern: `assert!(ready && matches!(r, Ok(_)))` now reads
  `relational_check` / weak, no stronger than its strongest conjunct read
  alone. A chain with a pinning `==` conjunct (`total == 4 && ..`) keeps its
  reading; a self-comparison (`value == value`) does not count as one.
  `prop_assert!(matches!(r, Ok(_)))` now reads its pattern like `assert!`
  (RIPR-SPEC-0231 rule 3, example 7;
  [#7695](https://github.com/EffortlessMetrics/ripr-swarm/issues/7695)).
  A snapshot inside a deciding `||` arm
  (`assert!(ready || { assert_snapshot!(v); true })`) no longer earns
  `snapshot` / medium.
