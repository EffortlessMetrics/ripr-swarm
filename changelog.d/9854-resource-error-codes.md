<!-- section: Fixed -->

- MCP `resources/read` failures no longer all surface as JSON-RPC `-32002`
  ResourceNotFound. Identity misses (`item_not_found`, `attempt_not_found`)
  keep `-32002` (upgraded to `-32602` for 2026-07-28 peers by the SDK);
  session-state and bound kinds (`no_snapshot`, `stale_snapshot`,
  `analysis_in_flight`, `workspace_unavailable`, `result_too_large`,
  `attempt_invalid`, `superseded`, `seam_not_found`) now answer InternalError
  (`-32603`), the closest standard JSON-RPC code. The human sentence, the
  structured recovery routes, and the stable typed state in `data.code` are
  unchanged for existing clients (#9854).
