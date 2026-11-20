<!-- section: Fixed -->
- The TypeScript guard-entry gate now reads `Array.isArray(param)` and strict
  `typeof param === 'lit'` / `!==` guards (either operand order) against literal
  call arguments, so a changed line inside such a branch no longer reads
  `exposed` from a test whose input provably skips it; non-literal arguments,
  loose `==` and rebound parameters stay unreadable and keep the verdict
  (#15261).
