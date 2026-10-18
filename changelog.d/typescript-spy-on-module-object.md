<!-- section: Changed -->
- A literal `toHaveBeenCalledWith` on a `jest.spyOn` / `vi.spyOn` of an
  exported top-level `const` module object now reads `exposed` when the
  owner's only call is the changed `object.method(...)` and the test body
  is only the spy, one owner call and the assertion. Every other shape
  (a helper, a getter read, a second spy, a hook, `each`, a module mock)
  keeps the medium reading. Row `jest-spec0247-ex05-spyon-send` moves
  from `gap` to `credited` (RIPR-SPEC-0247 rule 4, #6654).
