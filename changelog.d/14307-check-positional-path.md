<!-- section: Changed -->
- `ripr check src/lib.rs` scopes the run to that changed file, like
  `ripr check --path src/lib.rs`: a bare argument is a repository-relative
  file, directory or glob, and one that matches no changed file still fails
  closed with `scope_filter_matched_nothing`. Before, a bare argument was
  refused as unknown (#14307).
