<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0248): the dropped-term witness now reads the
  `local(...)` of an `import { owner as local }` rename, the outer call of
  `owner(owner(a, b), c)` and any non-zero tested input (no `1e-9`
  cutoff), so those discriminating tests keep `exposed`. When a
  change-specific rule withdraws `exposed`, the discriminate summary now
  says so instead of claiming discrimination. The zero-input shape is an
  evidence-promotion honesty case (`ts_dropped_term_zero_input`).
