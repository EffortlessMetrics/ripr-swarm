<!-- section: Fixed -->
- TypeScript `toMatchObject(expected)` now reads a test-body
  `const expected = { ... }` flat literal that the test names nowhere else,
  so the field pin credits a listed returned field instead of reporting a
  false `weakly_exposed` gap; any other mention, a `let` binding or a
  binding outside the test keeps the weak reading (RIPR-SPEC-0250, #6654).
