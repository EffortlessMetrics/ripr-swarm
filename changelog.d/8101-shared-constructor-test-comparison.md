<!-- section: Fixed -->
- A comparison or pattern compared with a non-literal side through the same
  `Some(..)`, `Ok(..)` or `Err(..)` on both sides
  (`assert_eq!(Some(r == Err(E::Bad)), Some(expected))`) no longer earns
  `exact_error_variant`/strong credit (RIPR-SPEC-0231 rule 1; #8101).
