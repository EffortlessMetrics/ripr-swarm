<!-- section: Added -->
- The LSP server binds the document version to `publishDiagnostics` for
  clients that negotiate `publishDiagnostics.versionSupport`, and to every
  workspace-pull diagnostic report, so stale deliveries are recognizable;
  push clients without negotiation see the previous version-less shape, and
  clears stay unversioned so they always apply (#6988).
