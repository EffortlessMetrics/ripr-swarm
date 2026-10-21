<!-- section: Fixed -->
- `ripr mcp --stdio` rejects a request whose JSON-serialized id exceeds 16384
  bytes with a bounded `-32600` error carrying `"id": null` and keeps the
  session alive, instead of exiting with `MCP output limit` when the echoed
  id could not fit the 128 KiB response cap, on both the valid-request and
  invalid-shape paths (#7768, #5254).
