<!-- section: Changed -->
- MCP: an empty committed `ripr_refresh` now carries `empty_scope_hint` (why, and
  the `base`/`source` to pass); not-ready repair cards and `ripr_prepare_repair`
  refusals carry `finding_guidance` (`missing`, `recommended_next_step`); and a
  card's CLI detail routes gain `ripr_get_gap` twins in `mcp_detail_routes`.
  Additive; no verdict or readiness change (#11213).
