<!-- section: Fixed -->
- A bare `x.unwrap()` or `x.expect("..")` setup statement is no longer listed
  as a separate "smoke only" related-test row beside the same test's real
  assertion, so the test reports its assertion alone and the matched-row
  counts and detail-line counts in `ripr check` human, `human-full` and JSON
  output shrink to match. A test whose only oracle is the unwrap or expect
  stays listed as smoke (#8386).
