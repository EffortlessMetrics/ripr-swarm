<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0249): `expect(x).toBeInstanceOf(C)` is now a
  recognised `relational_check` / weak matcher instead of an unresolved
  custom matcher. On a changed predicate it reads `exact_value` / strong
  when it observes the whole result of an owner call and the owner returns
  `C` on one path and, on another, a class its module shows is never a
  `C`, so `if (status === 404) return new NotFound(); return new
  Forbidden();` pinned by `toBeInstanceOf(NotFound)` and
  `toBeInstanceOf(Forbidden)` reads `exposed`. On the TypeScript verdict
  corpus false verdicts drop by one (#6654).
