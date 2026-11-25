<!-- section: Fixed -->
- MCP `ripr_refresh`, `ripr_list_gaps` and `ripr_workspace_status` now add
  `analysis_status: "not_analyzed"` and `clean: false` beside a zero-item
  empty-scope worktree snapshot too, so saved-worktree results that examined
  no behavior never read as a clean zero; the committed-specific
  `empty_scope_hint` stays committed-only (#13096).
