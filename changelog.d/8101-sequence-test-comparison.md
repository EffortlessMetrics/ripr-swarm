<!-- section: Fixed -->
- A comparison or pattern compared with a non-literal element inside two
  arrays or `vec!` literals of the same length
  (`assert_eq!(vec![r == Err(E::Bad)], vec![expected])`) no longer earns
  `exact_error_variant`/strong credit (RIPR-SPEC-0231 rule 1; #8101).
