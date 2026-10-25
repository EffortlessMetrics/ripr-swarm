<!-- section: Fixed -->
- Rust helper chains: a wrapper that rebinds the forwarded parameter (for
  example through a closure shadow) or invokes a macro whose expansion ripr
  cannot establish no longer lends the inner function's assertion credit to
  the wrapped predicate or return value. Only bare standard assertion and
  formatting macros stay transparent, and only while no file rebinds those
  names; path-qualified macros (including `tracing::`/`log::` logging calls,
  which a crate alias can redirect) withhold the credit (#10284).
