<!-- section: Changed -->
- MCP: a `ripr_prepare_repair` refusal for `seam_not_found` or `seam_ambiguous` now carries `next.stub_route`, the `ripr agent stub --at FILE:LINE --kind FAMILY` route `ripr check` and `ripr repair` already name, so prepare does not dead-end where the CLI does not (#11622).
