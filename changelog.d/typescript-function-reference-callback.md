<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0243 rule 12): `test("title", name)` with a
  same-file `function name() {...}` now reads that function's assertions as
  the test's, so a changed owner it calls reads `exposed` instead of
  `no_static_path`. The reference resolves only for a parameterless,
  non-generator declaration mentioned nowhere else except as a titled
  `test` / `it` callback; a call, reassignment, same-named binding, export,
  second declaration or `.each` / `.skip` form still refuses (#8938).
