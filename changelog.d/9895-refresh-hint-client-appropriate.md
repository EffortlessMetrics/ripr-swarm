<!-- section: Fixed -->
- LSP recovery hints no longer name the VS Code-only `ripr.refreshDiagnostics`
  command to other clients: the `recovery` field of `ripr/analysisStatus`
  component outcomes and the degradation log line name the server-executed
  `ripr.refresh (workspace/executeCommand)` route unless the client explicitly
  advertised `ripr.refreshDiagnostics` in `riprEditor.commands`, in which case
  the palette command is kept; a version-only, empty or unrelated-commands block
  gets the universal route. The VS Code extension now advertises that command
  (#9895).
