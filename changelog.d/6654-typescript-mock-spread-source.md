<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0247): `const db = { write: jest.fn(), ...defaults }`
  now keeps its mock when `defaults` is a top-level `const` object literal
  of the test file with plain `key: value` entries, no `write` key, and no
  other mention than the spread, so a literal
  `expect(db.write).toHaveBeenCalledWith(2)` reads `exposed` instead of
  `weakly_exposed`. A source with the key, a nested spread, a `let`
  binding, a mutation, a body-local rebinding or an `eval` still refuses.
  On the TypeScript verdict corpus false verdicts drop by one (#6654).
