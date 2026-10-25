<!-- section: Fixed -->
- Rust helper chains: a wrapper that rebinds the forwarded parameter (for
  example through a closure shadow) or invokes a user-defined macro before
  forwarding no longer lends the inner function's assertion credit to the
  wrapped predicate or return value. Standard assertion and formatting macros
  stay transparent; logging macros stay transparent only under a `tracing::`
  or `log::` path, since a bare `info!` may be a crate's own macro (#10284).
