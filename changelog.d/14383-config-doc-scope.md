<!-- section: Changed -->
- The configuration reference now says where `[severity.seams]` and
  `[suppressions] path` actually apply. Seam severities also drive SARIF,
  review-comments, agent-brief and agent results, and badge summaries.
  The suppressions file is also applied by `--format sarif` and
  `sarif-compact`. The `ripr init` template now shows the `[lsp]
  diagnostic_profile` setting as a commented default (#14383).
