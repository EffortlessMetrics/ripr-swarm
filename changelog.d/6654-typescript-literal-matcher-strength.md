<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0247): three matchers whose expected value is a
  literal in the test now read strong. `toHaveBeenCalledWith` with only
  primitive literal arguments on a `jest.fn()` / `vi.fn()` the test makes
  under an owner parameter's own name and passes in that position, when
  the changed line is the owner's only use of the parameter; legacy
  `node:assert` `equal` / `deepEqual` and chai `assert.equal` against a
  literal that `==` coercion cannot reach (not `0`, `1`, `""`, a numeric
  string, a boolean or `null`); and `toMatchInlineSnapshot` with literal
  text. On the TypeScript verdict corpus false verdicts drop from 17/101
  to 10/101: false actionable 16/62 to 9/62 (#6654).
