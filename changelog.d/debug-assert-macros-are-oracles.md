<!-- section: Fixed -->
- A test whose only check is `debug_assert!`, `debug_assert_eq!`,
  `debug_assert_ne!` or `debug_assert_matches!` is now read as an oracle by the
  parser path, so the change no longer reads `reachable_unrevealed` when such a
  check runs under `cargo test`'s debug profile. Pin and exactness credit stays
  keyed to the `assert*!` spellings, so these findings read `weakly_exposed`,
  never `exposed` (#8051 item 1).
