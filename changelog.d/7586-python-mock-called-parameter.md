<!-- section: Fixed -->
- Python: a changed call to a parameter (`handler(n + 1)`) is `exposed`
  when a test pins the mock it passed with `assert_called_once_with`,
  including through the owner's result when the owner returns that
  parameter. A rewrite that only wraps or unwraps an argument (`abs(n)`)
  no longer credits a pinned mock expectation, because the wrapper can
  return the same value at the test's input. The Python verdict corpus
  has 0 of 65 false verdicts (RIPR-SPEC-0238 rule 17, #7586).
