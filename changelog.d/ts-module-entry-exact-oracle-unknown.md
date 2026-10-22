<!-- section: Fixed -->
- TypeScript: a test that reaches a private helper only through an exported
  caller with an exact assertion now reads `static_unknown` (propagation
  evidence unknown) instead of `weakly_exposed` "add a test", since the
  caller's own code may carry the helper's effect to the assertion (#9290).
