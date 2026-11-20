<!-- section: Fixed -->
- `ripr doctor` now fails with an `output_location` check, and withholds the
  `ripr check` recommendation, when `ripr check` would refuse the default
  output tree: `target` is a regular file, or a link that dangles, loops or
  leaves the workspace. It used to pass, print `✓ Cache`, and call the state
  harmless. Setting `RIPR_CACHE_DIR` still waives it, as it does for
  `ripr check` (#14114).
  With a start-here packet present, `ripr doctor -v` now gives the recovery
  line instead of packet refresh advice while any `!` check blocks, matching
  the compact screen.
