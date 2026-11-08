<!-- section: Changed -->
- Diff analysis no longer rebuilds the main index from scratch when an
  admitted dependent file contains `include!`: the admission-core index is
  extended in place whenever that `include!` leaves every already-indexed
  file's include parent, include targets and unresolved-include state
  unchanged, and still falls back to a fresh build when it does not (#13754).
