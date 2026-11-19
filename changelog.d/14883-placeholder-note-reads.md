<!-- section: Fixed -->
- The "expected value is an unfilled placeholder macro" note no longer
  attaches to an assertion that only names the placeholder local in its
  message or a string literal, such as `assert_positive(price(100),
  "expected total")` next to `let expected = todo!()`. The note now uses the
  same read test as the rule that lowers the assertion, so a helper that
  reads the placeholder, directly or through `format!("{expected}")`, keeps
  it. Verdicts are unchanged (#14883).
