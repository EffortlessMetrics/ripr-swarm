<!-- section: Fixed -->
- TypeScript preview: Playwright specs of pure functions are read like plain
  ones. `expect.soft(...)` is an oracle, an awaited `test.step` body is walked,
  `toThrow(/^message$/)` pins an anchored message, and a moved threshold of a
  returned conditional is credited when a strong exact assertion calls the
  owner with a literal between the old and new threshold (#9434).
