<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0243 rule 13): a suite factory called more than once
  now keeps committed-snapshot credit for each call whose snapshot key is its
  own, no longer raises a false `typescript_test_extraction_partial` for the
  factory's own registration (a registration an expansion still drops stays
  disclosed, at the call's line), and points an expanded test's oracle
  evidence at the call instead of a line past it (#8955).
