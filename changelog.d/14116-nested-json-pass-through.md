<!-- section: Fixed -->
- A `serde_json::json!` call nested inside another one no longer makes the
  outer call count as a possible macro definition when the nested call reads
  a field named like a standard macro, such as `test.file` or `test.line`.
  Before, every trusted name the outer call mentioned, such as a `format!`
  beside the nested call, blocked matching assertions across the workspace,
  for example from ripr's own LSP command handler (RIPR-SPEC-0369, #14116).
