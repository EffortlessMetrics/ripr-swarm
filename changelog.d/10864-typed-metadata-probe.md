<!-- section: Internal / CI -->
- Preserve typed operational outcomes at the bounded offline Cargo metadata
  probe while keeping existing provider-identity callers fail-closed. The final
  output read is bounded by the existing 8-MiB limit; user-facing provider
  diagnosis and recovery remain a separate unfinished step (#10864).
