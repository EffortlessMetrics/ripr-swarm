<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0252): `t.assert.equal(...)`, `t.assert.strictEqual(...)`
  and the other `node:assert` methods on a `node:test` test context now read
  as assertions, with the same strength as legacy `node:assert`, when the
  test is registered through a `test` or `it` bound to `node:test`.
  `t.assert.equal(lateFee(10), 20)` reads `exposed` instead of
  `weakly_exposed` with no recognised assertion. On the TypeScript verdict
  corpus false verdicts drop by one (#6654).
