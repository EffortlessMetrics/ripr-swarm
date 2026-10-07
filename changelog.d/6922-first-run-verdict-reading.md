<!-- section: Changed -->
- `cargo xtask first-run` reads each verdict against the pinned crate's own
  tests: every case records the tests that fail with its edit applied, and the
  report marks `reachable_unrevealed` or `no_static_path` on a caught edit as a
  `false gap`, `weakly_exposed` there as understating the tests, and an unknown
  as `unresolved`. On the pinned crates, 0.10.0's `reachable_unrevealed`
  (semver) is a false gap and its `weakly_exposed` (bytesize) understates the
  tests; 0.11's `infection_unknown` on all three is unresolved, not a
  regression (#6922).
