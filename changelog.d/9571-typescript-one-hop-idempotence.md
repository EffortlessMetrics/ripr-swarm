<!-- section: Fixed -->
- TypeScript findings stop reading `exposed` when the only assertion compares
  an owner call with a local bound once to another (`const once =
  normalize(t); expect(normalize(once)).toEqual(once)`), because a constant
  return value satisfies it. A `let`, a reused local or any independent
  expected value keeps the credit (#9571, RIPR-SPEC-0248).
