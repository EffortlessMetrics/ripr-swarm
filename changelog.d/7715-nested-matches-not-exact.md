<!-- section: Fixed -->
- A `matches!` invocation with an unpinned pattern that is not the whole
  condition or a whole `&&` conjunct no longer earns `exact_value` / strong:
  `assert!(xs.iter().any(|x| matches!(x, Some(_))))` and
  `ensure!(matches!(r, Ok(_)) == true)` now read `relational_check` / weak.
  A `matches!` named only in a comment or string is no invocation and earns
  nothing, and a comment before the pattern's constructor
  (`matches!(x, /* , */ Some(_))`) no longer hides it
  ([#7772](https://github.com/EffortlessMetrics/ripr-swarm/issues/7772)). An invocation whose pattern pins a value (`Some(3)`), or a chain
  with a pinning `==` conjunct, keeps its exact reading (RIPR-SPEC-0231 rule 3,
  example 7; [#7715](https://github.com/EffortlessMetrics/ripr-swarm/issues/7715)).
