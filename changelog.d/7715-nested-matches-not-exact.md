<!-- section: Fixed -->
- A `matches!` that is not the whole condition or a whole `&&` conjunct no
  longer earns `exact_value` / strong whatever its pattern:
  `assert!(xs.iter().any(|x| matches!(x, Some(_))))` and
  `ensure!(matches!(r, Ok(_)) == true)` now read `relational_check` / weak,
  and a `matches!` named only in a comment or string earns nothing. A
  `matches!` whose pattern pins a value (`Some(3)`), or a chain with a
  pinning `==` conjunct, keeps its exact reading (RIPR-SPEC-0231 rule 3,
  example 7; [#7715](https://github.com/EffortlessMetrics/ripr-swarm/issues/7715)).
