<!-- section: Fixed -->
- LSP `ripr.collectWorkspaceStatus` no longer reports an opened changed
  document of a configured-but-disabled language as `clean`/`served` while
  the same payload names `language_adapter_unavailable`. Those rows use
  `not_analyzed` with reason `language_adapter_not_enabled` (distinct from
  #5998's `outside_analyzed_partition`) and recovery that names
  `[languages] enabled` plus a sidecar restart when the adapter is compiled
  in, appending the language's `enable_prerequisite` so compiled-in Perl
  still names the fact-packet/exporter requirement, or the language's
  rebuild/prerequisite guidance when the adapter is not compiled in.
  Matching uses the same path-text authority on advisory samples and
  document relatives, so a literal `%` in the filename cannot revive the
  clean/served lie. Rust is never a preview advisory: when the producer
  emits the rust-excluded-by-config limitation, opened `.rs` rows fail
  closed the same way rather than remaining `clean`/`served` (#7205).
