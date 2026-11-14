<!-- section: Added -->
- `ripr check` counts `no_static_path` findings whose only reaching code is a
  proof harness ripr does not evaluate. JSON carries `summary.proof_harness_only`
  (additive, emitted only when greater than 0, declared in
  `check.schema.json`), the human summary names the count and says it is not
  test coverage, and the MCP `ripr_list_gaps` document carries a top-level
  `proof_harness_only` count plus `"proof_harness_only": true` on each such
  item. The exposure classes are unchanged and the harness earns no test
  credit (#13085).
