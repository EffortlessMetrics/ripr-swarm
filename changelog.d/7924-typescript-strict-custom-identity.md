<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0246 / RIPR-SPEC-0262): a credited same-file strict
  custom matcher now carries reference identity on the assertion, so a
  copy-for-identity change observed by `expect(ready(cfg)).toBeSame(cfg)`
  reads `exposed` instead of `weakly_exposed`. `toEqual` still does not
  pin identity (`exact_value` is not enough). A refused or non-strict
  custom matcher, and a literal such as `toBeSame(null)`, stay
  `weakly_exposed` (#7924).
