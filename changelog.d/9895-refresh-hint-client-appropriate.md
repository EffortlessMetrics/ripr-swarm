<!-- section: Fixed -->
- LSP recovery hints no longer name the VS Code-only `ripr.refreshDiagnostics`
  command to other clients: the `recovery` field of `ripr/analysisStatus`
  component outcomes and the degradation log line name the server-executed
  `ripr.refresh (workspace/executeCommand)` route unless the client advertised
  the `riprEditor` extension block, in which case the palette command is kept
  (#9895).
