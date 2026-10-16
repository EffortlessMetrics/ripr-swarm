<!-- section: Fixed -->
- A comparison or `matches!` wrapped in the same `Some(..)`, `Ok(..)` or
  `Err(..)` constructor on both sides of an equality assertion and compared
  with a `false` literal or a non-literal
  (`assert_eq!(Some(matches!(r, Err(E::Bad))), Some(false))`) no longer earns
  `exact_value`/strong credit (RIPR-SPEC-0231 rule 1; #8101).
