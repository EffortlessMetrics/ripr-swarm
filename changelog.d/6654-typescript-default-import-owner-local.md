<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0251): a renamed default import of the owner now
  counts as the owner when ripr checks whether a strong assertion observes
  the changed value. `import cost from "../src/priceRenamed"` with
  `expect(cost(150)).toBe(140)` reads `exposed` instead of
  `weakly_exposed` with "observes an unrelated expression", when the owner
  is that module's default export and the test does not redeclare `cost`.
  On the TypeScript verdict corpus false verdicts drop by one (#6654).
