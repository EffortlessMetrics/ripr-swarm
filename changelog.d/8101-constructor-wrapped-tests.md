<!-- section: Fixed -->
- A test wrapped in an `Option`/`Result` constructor and compared with a
  wrapped non-literal or `false` side (`assert_eq!(Some(r == Err(E::Bad)),
  Some(expected))`, `assert_eq!(Ok(matches!(r, Err(E::Bad))), Ok(false))`) no
  longer earns `exact_value`/strong credit, while `Some(true)` still pins
  (RIPR-SPEC-0231 rule 1; #8101).
