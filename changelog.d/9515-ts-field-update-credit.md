<!-- section: Fixed -->
- The TypeScript preview adapter no longer reports a gap for a changed
  `this.f += e` / `this.f = this.f + e` statement that a strong exact assertion
  reads (`expect(stats.hits).toBe(1)` after `stats.recordHit()` on a fresh
  instance). The update is credited as a return-value change only when the
  method writes the field once on its straight-line path and the call always
  runs before the read; table and helper expected sides stay gaps (#9515).
