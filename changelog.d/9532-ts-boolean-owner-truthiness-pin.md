<!-- section: Fixed -->
- TypeScript preview: `assert.ok(isAdult(18))`, `assert.ok(!isAdult(17))` and
  `expect(isAdult(18)).toBeTruthy()` on an owner declared `: boolean` now count
  as exact pins when the call has literal arguments, instead of reading
  smoke-only and asking for an assertion that already exists (#9532).
