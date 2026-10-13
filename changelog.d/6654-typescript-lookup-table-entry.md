<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0254): a changed value in a one-line lookup table
  that the owner returns as `table[param]` now reads as a return-value
  change. It reads `exposed` only when a strong assertion looks up a
  changed key (`await expect(fetchUnitPrice("pear")).resolves.toBe(95)`),
  and otherwise `weakly_exposed`, naming the changed keys, instead of
  "no statically comparable predicate boundary". On the TypeScript verdict
  corpus false verdicts drop by one (#6654).
