<!-- section: Fixed -->
- TypeScript: an assertion written after a `try`/`finally` that always exits
  (`try { return; } finally { cleanup(); }`, `try { throw ... } finally {}`,
  or a `try` and `catch` that both exit) is dead code and no longer earns
  oracle credit, so the change reads `weakly_exposed` instead of `exposed`.
  Conditional exits and a `catch` that falls through keep the later credit
  (#9138).
