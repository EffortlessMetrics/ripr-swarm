<!-- section: Changed -->
- MCP `ripr_refresh` now marks an empty-scope snapshot of a committed comparison `analysis_status: "not_analyzed"` and `clean: false` in its primary `snapshot` fields, so a zero-finding default-base snapshot no longer reads as an all-clear. `clean` is only ever present as `false`; worktree snapshots keep their `worktree_scope_disclosure` and are unchanged (#13096).
