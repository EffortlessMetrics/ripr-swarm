<!-- section: Fixed -->
- Classified-cache publication for one key now holds an advisory file lock, so
  two ripr processes sharing a cache directory cannot interleave a late
  rollback with a newer commit and serve a stale classified result. A writer
  that cannot take the lock within two seconds skips with
  `skipped_publication_busy`; a cache that cannot lock publishes as before
  (#5350).
