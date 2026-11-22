<!-- section: Fixed -->
- A `serde_json::json!` call nested inside another one no longer makes the
  outer call count as a possible macro definition. Before, a `format!` inside
  the nested call blocked every `format!` assertion in the workspace, such as
  in ripr's own LSP command handler (RIPR-SPEC-0369, #14116).
