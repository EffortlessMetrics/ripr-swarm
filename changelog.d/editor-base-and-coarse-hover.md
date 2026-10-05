<!-- section: Fixed -->
- LSP: hovering a finding whose diagnostic has a coarse line-level origin now
  shows its evidence. Zero-width ranges (column precision refused) matched no
  hover position, so every hover on those lines fell back to the generic
  "Run `ripr check`" text. A zero-width range now covers its own line only,
  and a column-precise finding on the same line still wins at its columns.

- VS Code: `ripr.baseRef` defaults to empty, which resolves the repository
  default branch like `ripr check`, instead of `origin/main`. Repositories on
  `master` no longer fail their first editor refresh. When no default branch
  resolves, the status bar names `ripr.baseRef` as the fix.
