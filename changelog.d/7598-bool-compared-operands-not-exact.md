<!-- section: Fixed -->
- A comparison, disjunction or `matches!` that an assertion reads only
  through a boolean literal, a negation or an unread operator no longer
  earns an exact oracle: `assert_eq!(score(2) != 0, true)`,
  `assert_eq!(r == Err(E::Bad), false)`, `ensure!((a == b) == false)`,
  `assert!((matches!(r, Ok(3)) || f) == true)`,
  `assert_eq!(ready && matches!(r, Ok(_)), true)`,
  `assert!(matches!(r, Err(E::Bad)) ^ true)` and
  `assert!(std::ops::Not::not(matches!(r, Err(E::Bad))))` now read
  `relational_check` / weak. In a boolean assertion's `&&` chain only the
  conjunct holding such a `matches!` drops out, so `ensure!(check(matches!(r, Ok(3))) &&
  score == 4)` keeps the reading `score == 4` earns; in `assert_eq!` the
  whole compared operand caps. A comparison asserted true
  (`assert_eq!(r == Err(E::Bad), true)`) and a bare bool value or call
  (`assert_eq!(!is_valid(x), true)`) keep their exact readings
  (RIPR-SPEC-0231 rule 1, example 31;
  [#7598](https://github.com/EffortlessMetrics/ripr-swarm/issues/7598)).
