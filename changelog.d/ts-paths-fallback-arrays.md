<!-- section: Fixed -->
- TypeScript preview: a `compilerOptions.paths` fallback array such as
  `"@/*": ["generated/*", "src/*"]` now resolves the way `tsc` does, to the
  first target that exists, instead of leaving every import through that key
  `no_static_path`. A later target is used only when nothing at all sits at
  the earlier one; a declaration file, a directory, or two candidate files
  there keep the import unresolved and disclosed (#16002).
