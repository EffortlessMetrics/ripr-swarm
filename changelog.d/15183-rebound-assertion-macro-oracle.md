<!-- section: Fixed -->
- The repository seam inventory (`ripr check --format repo-exposure-json`) no
  longer reads an `assert_eq!` (or another standard assertion macro) as a
  strong oracle when the test's scope rebinds that name, for example with a
  test-local `macro_rules! assert_eq` that compares nothing. The related test
  is reported with an `unknown` oracle strength, so its seams are not
  `strongly_gripped`. The standard macro keeps its credit (#15183).
