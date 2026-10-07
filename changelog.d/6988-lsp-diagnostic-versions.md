<!-- section: Added -->
- The LSP server binds the document version to `publishDiagnostics` and
  workspace-pull diagnostics for clients that negotiate
  `publishDiagnostics.versionSupport`, so stale deliveries are
  recognizable; other clients see the previous version-less shape, and
  clears stay unversioned so they always apply (#6988).
