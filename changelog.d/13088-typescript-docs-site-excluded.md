<!-- section: Changed -->
- TypeScript/JavaScript diff and repo analysis now skip files under a `docs/`
  directory (for example a monorepo's `packages/docs` site), so a docs-site link
  list no longer leads `ripr check`. Repo mode counts them as skipped files, the
  workspace walk still indexes `docs/`, and a real library under `docs/` opts
  back in with `[languages.typescript] handwritten_files` (#13088).
