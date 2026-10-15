<!-- section: Fixed -->
- A test held in a value wrapper and compared with a wrapped value that is no
  boolean literal (`assert_eq!(Some(r == Err(E::Bad)), Some(expected))`,
  `Ok::<bool, ()>(..)`, a constructor, `Box::new(..)`, `[..]`, `vec![..]`, or
  `Some(..).unwrap()`) no longer earns `exact_value`/strong credit; the same
  test asserted true through `Some(true)` still pins (RIPR-SPEC-0231 rule 1;
  #8101).
