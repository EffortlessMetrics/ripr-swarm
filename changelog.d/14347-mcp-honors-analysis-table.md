<!-- section: Fixed -->
- `ripr_refresh` over MCP now applies `ripr.toml`'s `[analysis] mode` and
  `include_unchanged_tests`, the same way `ripr check` and the LSP do, so a
  coding agent and the CLI see the same findings on the same diff. The
  session block's `mode` reports the configured mode instead of always
  `draft` (#14347).
