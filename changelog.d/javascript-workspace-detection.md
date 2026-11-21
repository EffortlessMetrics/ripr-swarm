<!-- section: Fixed -->
- A JavaScript-only npm, yarn or pnpm workspace selects the TypeScript/JavaScript
  preview adapter with no `ripr.toml`: a `packages/*`, `apps/*`, `libs/*` or
  `libraries/*` package with a `package.json` and JavaScript source now counts,
  as a root shaped that way already did. Before, `ripr check` at the workspace
  root reported every changed JavaScript file as not analyzed (#15914).
