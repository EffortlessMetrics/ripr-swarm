<!-- section: Fixed -->
- LSP: hovering a finding whose diagnostic has a coarse line-level origin now
  shows its evidence. Zero-width ranges (column precision refused) matched no
  hover position, so every hover on those lines fell back to the generic
  "Run `ripr check`" text. A line-level range (zero-width, or the full-line
  span a coarse origin projects to) now covers every column of its own line
  only, and a zero-width one's hover highlights that line. A column-precise
  finding on the same line wins at its columns over a line-level finding.

- VS Code: `ripr.baseRef` defaults to empty, which resolves the repository
  default branch like `ripr check`, instead of `origin/main`. Repositories on
  `master` no longer fail their first editor refresh. When no default branch
  resolves, the status bar names `ripr.baseRef` as the fix (#5292).
