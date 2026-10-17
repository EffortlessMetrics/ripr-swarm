<!-- section: Fixed -->
- An assertion that indexes a table by a test
  (`assert_eq!(table[(r == Err(E::Bad)) as usize], expected)`) no longer earns
  `exact_error_variant`/strong credit (RIPR-SPEC-0231 rule 1; #8101).
