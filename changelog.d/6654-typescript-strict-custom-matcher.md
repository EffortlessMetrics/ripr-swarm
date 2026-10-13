<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0259): a custom matcher the test file registers with
  `expect.extend`, whose `pass` is a `&&` chain holding
  `received === expected` (or `Object.is(received, expected)`), now reads
  as a strong exact-value oracle, so `expect(toCents(1.5)).toBeCents(150)`
  reads `exposed` instead of `weakly_exposed`. A matcher registered in a
  hook, helper or other module, through a spread or twice, one with another
  body shape, or an assertion with `.not`, `.rejects` or other than one
  argument keeps the `unknown` reading. On the TypeScript verdict corpus
  false verdicts drop by one (#6654).
