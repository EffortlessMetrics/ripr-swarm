<!-- section: Changed -->
- MCP: `tools/list` is slim by default, cut from about 46 KB to about 11 KB per session start. The per-tool `outputSchema` documents are omitted unless the host sets `RIPR_MCP_TOOLS_LIST=full`, and the ten tool descriptions keep only purpose, when to call, the safety boundary and failure codes. Results are unchanged (#13803).
