<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0249): `expect(x).toBeInstanceOf(C)` is now a
  recognised `relational_check` / weak matcher instead of an unresolved
  custom matcher. On a changed `if` it reads `exact_value` / strong when
  it observes the whole result of an owner call and the `if`'s two
  branches return `new C(...)` and `new D(...)`, where the owner's module
  shows a `D` is never a `C`. So `if (status === 404) { return new
  NotFound(); } return new Forbidden();` pinned by
  `toBeInstanceOf(NotFound)` and `toBeInstanceOf(Forbidden)` reads
  `exposed`. On the TypeScript verdict corpus false verdicts drop by one
  (#6654).
