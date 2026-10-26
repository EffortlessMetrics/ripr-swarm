<!-- section: Fixed -->
- `ripr doctor` now warns on its default screen (and in `binary.warnings` of `--json`) when no `ripr` is on PATH but `.mcp.json`, `.vscode/mcp.json` or `.cursor/mcp.json` launches it by bare name; the advisory never changes the exit status and stays quiet otherwise (#10347).
