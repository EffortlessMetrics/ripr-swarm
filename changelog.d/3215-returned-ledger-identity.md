<!-- section: Fixed -->
- Rust helper diagnostics retain a bounded mutable-argument/returned-local
  ledger identity instead of describing that ledger as a dropped helper result.
  Exact indexed field observation remains an explicit unknown (#3215).
- A proved first-hop ledger no longer hides an unproved or dropped result in
  a higher observed wrapper; that route retains its forwarding limitation.
