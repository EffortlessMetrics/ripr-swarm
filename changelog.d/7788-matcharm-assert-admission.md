<!-- section: Fixed -->
- Match-arm evidence refuses shadowed or skipped plain `assert!` invocations
  through the shared assertion execution checks; current-generation output
  examples and corpus fixtures track its cache invalidation. Real-producer
  xtask and LSP generation expectations are checked against the live cache
  identity to prevent stale consumer pins (#7788, #7809).
