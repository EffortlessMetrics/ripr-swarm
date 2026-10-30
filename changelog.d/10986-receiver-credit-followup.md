<!-- section: Fixed -->
- The typed-parameter receiver credit scans the test file for rebound owner names
  at most once per probe, and accepts exactly one `&` or `&mut` borrow
  (`&&Site`, `&mut &Site` and `&'a Site` are refused), now pinned by tests.
  Follow-up to #11000, #10986.
