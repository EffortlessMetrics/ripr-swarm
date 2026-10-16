<!-- section: Fixed -->
- A test wrapped in the same `Some(..)`, `Ok(..)` or `Err(..)` constructor on
  both sides of an equality (`assert_eq!(Some(r == Err(E::Bad)), Some(expected))`,
  `assert_eq!(Some(matches!(r, Err(E::Bad))), Some(false))`) no longer earns
  `exact_error_variant`/strong credit; the wrapper compares its contents, so it
  reads as the bare sides do (RIPR-SPEC-0231 rule 1; #8101).
