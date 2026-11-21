<!-- section: Fixed -->
- TypeScript: Jasmine `toBeTrue()` / `toBeFalse()` now read as exact boolean
  oracles (the same strength as `toBe(true)` / `toBe(false)`) instead of an
  unrecognised matcher, `.not.` forms cap like `.not.toBe(true)`, and
  `toHaveSize(n)` reads like `toHaveLength(n)` (#15627).
