<!-- section: Changed -->
- `ripr init --mcp` (and `--agents cursor --mcp`) now writes `"env": {"RIPR_MCP_TOOLS_LIST": "slim"}` in the generated ripr MCP entry, cutting about 31 KB (roughly two thirds) from the `tools/list` every agent session pays at start; results are unchanged and deleting the line restores the full listing (#13050).
