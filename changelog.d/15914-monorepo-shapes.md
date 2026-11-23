<!-- section: Fixed -->
- TypeScript: a tsconfig `paths` target that names a directory resolves to that
  directory's `index` file, as the compiler does, so the monorepo idiom
  `"@acme/*": ["packages/*/src"]` relates a test in one package to the owner it
  imports from another. Before, the import stayed unresolved and a boundary
  the sibling package's test already checked was reported as a missing check.
  A directory with its own `package.json`, or with a same-named file beside
  it, still resolves nothing, and so does any directory target under
  `node16`, `nodenext` or `classic` module resolution (#15914).
- A JavaScript-only npm, yarn or pnpm workspace selects the TypeScript/JavaScript
  preview adapter with no `ripr.toml`: a `packages/*`, `apps/*`, `libs/*` or
  `libraries/*` package with a `package.json` and JavaScript source now counts,
  as a root shaped that way already did. Before, `ripr check` at the workspace
  root reported every changed JavaScript file as not analyzed (#15914).
- The `[languages] enabled` list that `ripr check` and `ripr doctor` print when
  a preview language is off keeps `rust` in a mixed repository whose crate sits
  below the root (`backend/Cargo.toml`, `crates/core/Cargo.toml`). Before, the
  list named only the preview languages, and pasting it switched off the Rust
  analysis the same run had just produced (#15914).
