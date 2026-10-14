<!-- section: Fixed -->
- Method-call, cast, block and combinator spellings of a bool-compared
  comparison no longer earn an exact oracle:
  `assert_eq!(r.eq(&Err(E::Bad)), false)`, `assert_eq!(score(2).ne(&0), true)`,
  `assert_eq!(r == Err(E::Bad), false as bool)`,
  `assert_eq!({ r == Err(E::Bad) }, false)`,
  `assert_eq!((r == Err(E::Bad)).eq(&false), true)`,
  `assert_eq!(PartialEq::eq(&r, &Err(E::Bad)), false)` and
  `assert!(matches!(r, Err(E::Bad)).then(|| false).unwrap_or(true))` now read
  `relational_check` / weak. An `eq` method or a literal spelling asserted
  true (`assert_eq!(r.eq(&Err(E::Bad)), true)`) keeps its exact reading
  (RIPR-SPEC-0231 rule 1, example 32;
  [#7931](https://github.com/EffortlessMetrics/ripr-swarm/issues/7931)).
