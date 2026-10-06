<!-- section: Changed -->
- `cargo xtask first-run` reads each verdict against the pinned crate's own
  tests: every case records the tests that fail with its edit applied, and the
  report marks a gap class on a caught edit as a `false gap` and an unknown as
  `unresolved`. On the pinned crates, 0.10.0's `reachable_unrevealed` (semver)
  and `weakly_exposed` (bytesize) are false gaps; 0.11's `infection_unknown`
  on all three is unresolved, not a regression.
