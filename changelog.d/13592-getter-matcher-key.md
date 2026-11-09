<!-- section: Fixed -->
- TypeScript checks: a class getter observed only as a literal key of
  `expect(instance).toMatchObject({ getter: value })` or
  `expect(instance).toHaveProperty("getter", value)` on a receiver built by
  exactly `new Class(...)` now reads `weakly_exposed` instead of
  `no_static_path`, never `exposed` and with no repair start; `.not`,
  `.resolves`, `.rejects`, non-literal values and rebound or shadowed receivers
  earn no credit (#13592).
