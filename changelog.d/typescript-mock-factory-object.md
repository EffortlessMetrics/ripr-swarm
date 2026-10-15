<!-- section: Fixed -->
- TypeScript `toHaveBeenCalledWith` payloads on `db.write` now credit a mock
  object built by a test-file factory (`const db = makeDb();` where
  `function makeDb() { return { write: jest.fn() }; }`), as they already did
  for the inline literal, instead of reporting a false `weakly_exposed` gap.
  A factory with parameters, extra statements, an export, a spread or a
  `write` value other than a bare `jest.fn()`, or a file that rebinds or
  passes on the factory, keeps the medium reading
  (RIPR-SPEC-0247, #6654).
