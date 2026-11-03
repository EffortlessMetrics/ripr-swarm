<!-- section: Fixed -->
- A builder chain that applies a non-setter method after a verified setter
  (`RetryBuilder::new().attempts(5).reset().build()`) no longer counts as an
  override of that field, so the read is not treated as a false
  `weakly_exposed` gap. A method before the setter, and the conventional
  zero-argument `.build()` terminal, keep the existing override reading
  (#7557).
