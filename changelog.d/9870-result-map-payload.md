<!-- section: Fixed -->
- Rust owner-return pins recognize a bounded `Ok(f(self.read()?, ...))` to
  `self.read().map(|v| f(v, ...))` rewrite while refusing discarded callback
  payloads, unresolved reader or standard-name identity, and preserving each
  test's observation provenance (#9870).
