<!-- section: Changed -->
- TypeScript/JavaScript diff analysis now skips changed files under a `docs/`
  directory (for example a monorepo's `packages/docs` site), so a docs-site link
  list no longer leads `ripr check`. They are reported as not analyzed, the
  workspace walk still indexes `docs/`, and a real library under `docs/` opts
  back in with `[languages.typescript] handwritten_files` (#13088).
