<!-- section: Fixed -->
- Rust helper chains: a wrapper that rebinds the forwarded parameter (for
  example through a closure shadow) or invokes a user-defined macro before
  forwarding no longer lends the inner function's assertion credit to the
  wrapped predicate or return value. Standard assertion, formatting and
  logging macros stay transparent (#10284).
