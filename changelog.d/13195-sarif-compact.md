<!-- section: Added -->
- `ripr check --format sarif-compact` and `--format repo-sarif-compact`
  render the SARIF 2.1.0 documents minified for machine and agent
  consumers; parsed values are identical to `sarif`/`repo-sarif`,
  including the `RIPR_SARIF_RESULTS_BYTES` bound and its disclosed
  truncation (#13195).
