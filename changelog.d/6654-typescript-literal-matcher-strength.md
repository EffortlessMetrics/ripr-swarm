<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0247): three matchers whose expected value is a
  literal in the test now read strong. `toHaveBeenCalledWith` with only
  literal arguments on a `jest.fn()` / `vi.fn()` made in the test, which
  the owner calls once through a parameter on the changed line; legacy
  `node:assert` `equal` / `deepEqual` and chai `assert.equal` against a
  literal that `==` coercion cannot reach (not `0`, `1`, `""`, a numeric
  string, a boolean or `null`); and `toMatchInlineSnapshot` with literal
  text. On the TypeScript verdict corpus false verdicts drop from 17/101
  to 9/101: false actionable 16/62 to 8/62 (#6654).
