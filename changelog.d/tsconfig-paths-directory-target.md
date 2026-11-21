<!-- section: Fixed -->
- TypeScript: a tsconfig `paths` target that names a directory resolves to that
  directory's `index` file, as the compiler does, so the monorepo idiom
  `"@acme/*": ["packages/*/src"]` relates a test in one package to the owner it
  imports from another. Before, the import stayed unresolved and a boundary
  the sibling package's test already checked was reported as a missing check.
  A directory with its own `package.json`, or with a same-named file beside
  it, still resolves nothing (#15914).
