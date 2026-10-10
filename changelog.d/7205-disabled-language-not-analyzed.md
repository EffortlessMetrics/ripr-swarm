<!-- section: Fixed -->
- LSP `ripr.collectWorkspaceStatus` no longer reports an opened changed
  document of a configured-but-disabled language as `clean`/`served` while
  the same payload names `language_adapter_unavailable`. Those rows use
  `not_analyzed` with reason `language_adapter_not_enabled` (distinct from
  #5998's `outside_analyzed_partition`) and recovery that names
  `[languages] enabled` plus a sidecar restart (#7205).
