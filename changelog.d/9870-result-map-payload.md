<!-- section: Fixed -->
- Rust owner-return pins recognize a bounded `Ok(f(self.read()?, ...))` to
  `self.read().map(|v| f(v, ...))` rewrite while refusing discarded callback
  payloads and preserving each test's observation provenance (#9870).
