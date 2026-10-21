<!-- section: Fixed -->
- A multi-line builder-chain setup statement ending in `.unwrap()` or
  `.expect("..")` is no longer listed as a "smoke only" related-test oracle
  beside the same test's real `assert_eq!`; the assertion keeps its own grade.
  A test whose only oracle is the chain stays smoke (#9122).
