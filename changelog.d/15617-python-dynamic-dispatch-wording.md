<!-- section: Fixed -->
- The `dynamic_dispatch` limitation no longer explains itself in Rust terms on
  Python, TypeScript and Perl findings. The reason read "trait objects or
  virtual calls" and the next step named a "trait object or `dyn` value"; both
  now say the call target is chosen at runtime, and the next step asks for a
  test that calls the changed owner directly by its own name. The
  `dynamic_dispatch_unresolved` stop reason's next step changed the same way.
  Tokens, classes and exit codes are unchanged (#15617).
