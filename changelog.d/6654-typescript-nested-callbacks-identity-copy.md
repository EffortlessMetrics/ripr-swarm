<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0246): a change that only swaps a returned object
  for a copy of itself (`return cfg` to `return { ...cfg }`) no longer reads
  `exposed` from a structural matcher such as `toEqual`; it needs `toBe`,
  `strictEqual` or `t.is`. Assertions inside a literal-array `forEach`, a
  returned or awaited `.then` callback and an awaited `t.test` subtest are
  now read as the test's own. On the TypeScript verdict corpus false
  verdicts drop from 21/101 to 17/101: false actionable 19/62 to 16/62,
  false exposed 2/39 to 1/39 (#6654).
