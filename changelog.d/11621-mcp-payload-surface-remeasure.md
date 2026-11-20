<!-- section: Docs -->
- MCP: the slim `tools/list` section of the interoperability guide now states
  the advertised schema count correctly (ten tools after #11911 added the
  repair-attempt inventory; the sentence was left at nine) and carries the
  measured wire sizes, and a check reads the sentence back against the live
  payload so the count cannot drift again; the recorded `$defs`/`$ref`
  factoring rejection in `mcp/protocol.rs` is refreshed with the same
  re-measurement (#11621).
