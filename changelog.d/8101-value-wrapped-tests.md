<!-- section: Fixed -->
- A test wrapped in `Some(..)`, `Ok(..)`, `Err(..)`, a one-element array or an
  `.unwrap()`/`.expect(..)` on such a constructor and compared with a
  non-literal (`assert_eq!(Some(r == Err(E::Bad)), Some(expected))`) no longer
  earns `exact_value`/strong credit (RIPR-SPEC-0231 rule 1; #8101).
