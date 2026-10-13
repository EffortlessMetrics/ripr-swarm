<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0250): `expect(x).toMatchObject(o)` is now a
  recognised `relational_check` / weak matcher instead of an unresolved
  custom matcher. On a changed field of the one object literal the owner
  returns, it reads `exact_value` / strong when it observes the whole
  result of an owner call and `o` is a flat literal that lists that field.
  So `adult: age >= 18` changed in `return { name, adult: ..., tags }` and
  pinned by `toMatchObject({ adult: false })` at 17 and `{ adult: true }`
  at 18 reads `exposed`, as the same test written with `toEqual` already
  did. On the TypeScript verdict corpus false verdicts drop by one (#6654).
