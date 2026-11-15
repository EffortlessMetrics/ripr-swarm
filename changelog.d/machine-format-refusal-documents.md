<!-- section: Fixed -->
- A refused `ripr check` run (unresolvable base, invalid `ripr.toml`,
  unusable root) now prints a refusal document on stdout in every machine
  format, not only `json`, `sarif`, `github`, `missed-txt`, `badge-json`,
  `repo-exposure-json` and `agent-seam-packets-json`: `json-terse` and
  `check-findings-json` print the check JSON refusal envelope, every Shields
  and native badge variant prints the degraded `refused: <identity>` badge,
  `repo-sarif` prints the refusal SARIF run, and `repo-seams-json` and
  `repo-exposure-summary-json` print their own `run_status: "refused"`
  envelopes. Before, these wrote zero bytes (#14499).
