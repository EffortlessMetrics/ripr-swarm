<!-- section: Fixed -->
- Python: a changed call to a parameter (`handler(n + 1)`) is `exposed`
  when a test pins the mock it passed with `assert_called_once_with`,
  including through the owner's result when the owner returns that
  parameter. An argument rewrite that keeps the old argument inside it
  (`abs(n)`, `max(n, 0)`, `n or 0`) no longer credits a pinned mock
  expectation, because it can return the same value at the test's input;
  literal arithmetic (`n + 1`, `n * 100`) still does. The Python verdict corpus
  has 0 of 65 false verdicts (RIPR-SPEC-0238 rule 17, #7586).
