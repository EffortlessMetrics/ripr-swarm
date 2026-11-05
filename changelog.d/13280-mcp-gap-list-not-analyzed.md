<!-- section: Fixed -->
- MCP gap lists now explicitly report `not_analyzed` and `clean: false`
  for committed zero-item empty-scope snapshots; a nonempty analysis or a
  filtered-to-zero page remains distinct. The closed gap-list schema advances
  to v5 and keeps typed refusals and pagination intact (#13280).
