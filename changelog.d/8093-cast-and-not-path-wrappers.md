<!-- section: Fixed -->
- A test carried as an integer through a cast or `From`
  (`assert_eq!((r == Err(E::Bad)) as u8, 0)`,
  `assert_eq!(u8::from(r == Err(E::Bad)), n)`) and a `Not::not(..)` path
  call over a whole condition holding a `matches!`
  (`assert!(Not::not(flag || matches!(..)))`) no longer earn
  `exact_value`/strong credit (RIPR-SPEC-0231 rule 1; #8093, #8101).
