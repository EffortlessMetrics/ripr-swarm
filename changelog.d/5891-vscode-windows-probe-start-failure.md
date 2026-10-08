<!-- section: Fixed -->
- The VS Code extension reports `could not start` with the real cause when a
  configured `ripr.server.path` names a file that cannot start on Windows,
  instead of misnaming it an LSP compatibility failure with exit code 0, and a
  failed configured path no longer suggests setting `ripr.server.path` itself
  (#5891).
