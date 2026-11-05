<!-- section: Added -->
- `ripr check --format sarif-compact` and `--format repo-sarif-compact`
  render the SARIF 2.1.0 documents minified under the `RIPR_SARIF_BYTES`
  byte budget (default 1,000,000, `0` removes the cap); over-budget runs
  retain a result prefix and disclose the bound in-band as
  `limited_sarif_bound`, never as a clean run (#13195).
