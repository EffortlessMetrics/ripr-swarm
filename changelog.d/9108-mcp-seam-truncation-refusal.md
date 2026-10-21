<!-- section: Fixed -->
- `ripr_prepare_repair`, `ripr_list_gaps` and `ripr_get_gap` no longer call an
  item repair-ready when the repo-wide seam inventory is truncated by the seam
  limit (or cannot be established), the state in which `ripr repair` refuses
  every item: MCP now returns `repair_packet_ready: false` with the typed
  `seam_inventory_truncated` reason and no `awaiting_edit` attempt (#9108).
