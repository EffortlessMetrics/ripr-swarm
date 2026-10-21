<!-- section: Fixed -->
- `ripr_prepare_repair`, `ripr_list_gaps` and `ripr_get_gap` no longer call an
  item repair-ready when the shared repair gate behind `ripr repair` refuses
  its seam (for example a match arm whose existing tests already grip it):
  the MCP readiness now conjoins that validator's verdict, returns
  `repair_packet_ready: false` with a typed `seam_already_gripped`,
  `seam_gate_refused`, `seam_ambiguous` or `seam_not_found` reason and no
  `awaiting_edit` attempt (#8717).
