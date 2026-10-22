<!-- section: Fixed -->
- Candidate admission no longer counts the lines of generated `package-lock.json`
  and `Cargo.lock` files toward the ordinary size envelope (exact file-name
  match; hand-written lines and the file-count limit are unchanged), so a
  multi-lockfile fixture or dependency refresh is not refused as oversized
  (#9261).
  `changed_lines` stays the raw GitHub total and a new
  `counted_changed_lines` carries the figure compared with the limit; the
  discount is dropped if the PR head moves while the file list is read.
