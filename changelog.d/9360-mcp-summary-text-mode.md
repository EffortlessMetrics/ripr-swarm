<!-- section: Added -->
- MCP: `RIPR_MCP_TEXT_CONTENT=summary` (set in the host's server `env`) makes
  `content[0].text` of a successful tool result a short digest instead of a
  copy of `structuredContent`, cutting the response an agent pays for on hosts that read
  `structuredContent` by up to roughly half (`ripr_get_gap`). The default is unchanged: the text
  stays the compact JSON copy, so text-only hosts lose nothing, and
  `structuredContent` is the identical complete result in both modes (#9360).
