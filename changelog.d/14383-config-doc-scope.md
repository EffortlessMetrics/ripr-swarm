<!-- section: Changed -->
- The configuration reference now matches what the code reads. It says where
  `[severity.seams]` and `[suppressions] path` actually apply (seam severities
  also drive SARIF, review-comments and agent results; the suppressions file
  is also applied by `--format sarif` and `sarif-compact`), that
  `[profiles.bun_ub]` is read only by `ripr doctor`, that SARIF, `ripr context`
  and `missed-txt` also follow `[severity.findings]`, and that
  `analysis.production_like_targets` takes source file paths. The `ripr init`
  template now shows the `diagnostic_profile` setting under `[lsp]` as a
  commented default (#14383, #14397).
