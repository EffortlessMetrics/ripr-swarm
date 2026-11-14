<!-- section: Fixed -->
- The language server now refreshes when a file it analyzed is deleted,
  renamed or rewritten outside the editor while that file is not open, so
  diagnostics no longer stay on a file that is gone until a manual refresh.
  It registers one change-and-delete watcher for source files with clients
  that support dynamic watched-file registration, and an event for a Rust
  source whose bytes are unchanged schedules nothing (#14099).
