<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0262): a custom matcher the test file registers with
  `expect.extend`, whose `pass` is a `&&` chain holding
  `received === expected`, now reads
  as a strong exact-value oracle, so `expect(toCents(1.5)).toBeCents(150)`
  reads `exposed` instead of `weakly_exposed`. A matcher registered in a
  hook, helper or other module, through a spread or twice, one with another
  body shape, a file that may reach `expect` without naming it (an escape,
  a computed member, an alias, a namespace or dynamic import, or the global
  object), or an assertion with `.not`, `.rejects` or other than one
  argument keeps the `unknown` reading. On the TypeScript verdict corpus
  false verdicts drop by one (#6654).
