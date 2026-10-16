<!-- section: Fixed -->
- A boolean test wrapped in the same constructor on both sides of an equality
  macro (`assert_eq!(Some(r == Err(E::Bad)), Some(expected))`,
  `assert_eq!(Some(matches!(r, Err(E::Bad))), Some(false))`, and the `Ok(..)`
  and `Err(..)` spellings) no longer earns `exact_value`/strong credit; it
  caps like the unwrapped comparison does. A wrapped value that is no test
  (`Some(compute(x))`) still pins (RIPR-SPEC-0231 rule 1; #8101).
