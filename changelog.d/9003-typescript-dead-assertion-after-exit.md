<!-- section: Fixed -->
- TypeScript: an assertion written after an unconditional `return`,
  `throw`, `break` or `continue` in a test body (dead code that never runs)
  no longer earns oracle credit, so a change it "covers" reads
  `weakly_exposed` instead of `exposed`. Assertions before the exit and
  after a conditional exit (`if (skip) return;`) keep their credit (#9003).
