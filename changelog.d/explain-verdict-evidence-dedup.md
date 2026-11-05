<!-- section: Changed -->
- `ripr explain` no longer renders the same fact twice: each related test's
  assertion lives once in its Evidence row, the "Why this verdict" block
  references that row ("see Evidence above") instead of re-listing the
  assertion and miss sentence, and a test judged by several assertion rows
  states a repeated miss sentence once per test rather than once per row.
  Rows beyond the Evidence window keep their full verdict and checked
  assertion there, so no fact is lost; verdicts, classes and next steps are
  unchanged. Measured on the agent-efficiency study's scenario subjects, the
  explain output shrank 7.7-12.8% (mean 3,881 -> 3,461 bytes) with every
  needed-subset fact still present.
