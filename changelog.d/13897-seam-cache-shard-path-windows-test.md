<!-- section: Internal / CI -->
- The seam-cache reference-budget test resolves its expected shard path
  through the loader's own resolver, so the path it looks for in the load
  reason is spelled with native separators on Windows too (#13897).
