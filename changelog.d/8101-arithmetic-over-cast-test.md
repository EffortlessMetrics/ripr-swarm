<!-- section: Fixed -->
- Arithmetic over an integer-cast test and another non-literal term
  (`assert_eq!(k * (r == Err(E::Bad)) as u32, n)`) no longer earns
  `exact_error_variant`/strong credit (RIPR-SPEC-0231 rule 1; #8101).
