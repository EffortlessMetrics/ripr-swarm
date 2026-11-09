<!-- section: Changed -->
- The `ripr agent card` human renderers moved from the CLI adapter to `output::agent_card`, so `app` no longer imports `cli`; `policy/architecture.txt` now rejects `app` imports of `cli`, `lsp` and `mcp`, with four scoped cfg(test) exceptions (#5193).
