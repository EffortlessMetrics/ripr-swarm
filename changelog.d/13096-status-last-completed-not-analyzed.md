<!-- section: Fixed -->
- `ripr_workspace_status` no longer reports a committed empty-scope snapshot as a bare
  `finding_count: 0`: `last_completed_snapshot` now carries `analysis_status:
  "not_analyzed"` and `clean: false`, matching `ripr_refresh` and `ripr_list_gaps`
  (#13096).
