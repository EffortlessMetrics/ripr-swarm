<!-- section: Changed -->
- MCP `ripr_refresh` now marks an empty-scope snapshot `analysis_status: "not_analyzed"` and `clean: false` in its primary `snapshot` fields, so a zero-finding default-base snapshot no longer reads as an all-clear (#13096).
