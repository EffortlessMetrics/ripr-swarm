<!-- section: Fixed -->
- `ripr lsp` now warns in the log and, for generic editors, on screen when a
  `workspace/configuration` pull rejects settings such as
  `diagnosticProfile: "loud"` or `seamDiagnostics: "yes"`, naming every
  rejected setting with its accepted values; the previous settings stay in
  effect. Ignored initialization options also name their accepted values
  (#15312).
