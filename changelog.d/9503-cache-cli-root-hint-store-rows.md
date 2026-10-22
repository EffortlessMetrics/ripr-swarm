<!-- section: Fixed -->
- `ripr cache status` and `ripr cache clear` accept `--root PATH`, so after
  `ripr check --root X` from another directory they report and clear X's cache
  instead of the current directory's project (#9503).
- The `cache status` cleanup hint now depends on whether `cache clear` would
  accept the resolved cache directory: a relative, symlinked or otherwise
  refused `RIPR_CACHE_DIR` gets the refusal reason instead of a `clear --force`
  command that fails (#9503).
- File-fact cache store failures are named in ordinary output: `ripr rerun`
  JSON gains `cache.store_failures` (`{path, stage, error}` rows, at most 32)
  and `cache.store_failures_dropped`, and the analyzer prints a bounded stderr
  summary without `RIPR_REPO_EXPOSURE_LATENCY_TRACE` (#9503).
- `cargo xtask repo-exposure-latency-report` no longer reports `pass` from exit
  codes alone: cache store errors, ignored corrupt entries, an unobserved cache
  phase or unchecked output parity yield `warn` with named `status_reasons`
  (report schema 0.4), and a `:` in a Unix file name no longer invalidates the
  cache receipt (#9503).
