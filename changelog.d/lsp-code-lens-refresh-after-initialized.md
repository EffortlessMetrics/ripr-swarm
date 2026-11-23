<!-- section: Fixed -->
- `ripr lsp` no longer attempts `workspace/codeLens/refresh` while it is still
  handling `initialize`, so a session no longer starts with a "code lens
  refresh request failed: Server not initialized" warning (RIPR-SPEC-0138,
  #15431).
