<!-- section: Fixed -->
- A file-backed `insta` snapshot with no committed `.snap` file no longer closes a Rust return seam. The
  assertion fails on its first run, so it cannot notice a wrong value; the seam reads `weakly_gripped`
  like the field seam already did. Only the plain single-assertion shape is judged, and only when no
  tracked `.snap` file carries the test's snapshot name; redirected, repeated, parameterized or inline
  snapshots keep their grade. `.snap` files are now an analysis-cache input, and an untracked one does not
  credit a `--worktree` run (#12189).
