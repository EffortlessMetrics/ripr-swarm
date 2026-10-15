<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0258): a Jest `toMatchSnapshot("cap")` call now
  reads as a strong oracle when its committed entry `<test title>: cap 1`
  is in `__snapshots__/<test file>.snap`, so a changed return value reads
  `exposed` instead of `weakly_exposed`. Only a single plain string-literal
  hint (no template, `$`, backslash, or empty hint) is read, the entry
  must match exactly once with no second entry, and a test whose title the
  hinted key extends refuses. Vitest hints keep the medium reading. On the
  TypeScript verdict corpus `jest-spec0258-ex04-hint` moves from `gap` to
  `credited` (#NNNN).
