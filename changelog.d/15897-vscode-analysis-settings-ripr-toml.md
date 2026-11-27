<!-- section: Fixed -->
- The VS Code extension sends `ripr.check.mode` and `ripr.includeUnchangedTests`
  to the server only when a VS Code settings layer sets them, so `ripr.toml`
  `[analysis]` `mode` and `include_unchanged_tests` apply in the editor instead
  of being overridden by the extension's defaults (#15897).
