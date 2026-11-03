<!-- section: Changed -->
- MCP: a `superseded` repair-attempt or receipt read for a session transaction still in memory now carries `data.canonical_id`, `data.item_in_current_snapshot` and a `data.next` route (re-prepare the same item, or list the current items), so prepare, edit, refresh does not end in a dead failure (#11622).
