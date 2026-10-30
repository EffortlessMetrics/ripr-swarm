<!-- section: Added -->
- `ripr doctor` now checks a root's `.mcp.json`: a `ripr` server entry running
  `ripr mcp --stdio` (what `ripr init --mcp` writes) prints a `✓` line, and a
  missing entry, a different command or invalid JSON prints a `~` line naming
  the problem; `--json` carries the same verdict as an `mcp_config` check, and
  the exit status is unchanged (#10347).
