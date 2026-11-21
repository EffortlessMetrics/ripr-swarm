<!-- section: Fixed -->
- `ripr check` no longer treats files in an installed dependency tree that
  `.gitignore` does not cover (for example `node_modules/`) as unanalyzed
  source: it keeps offering the TypeScript or Python repair start and stops
  advising `git add -N` for files no adapter reads. When such a tree is too
  large for the repair's exact snapshot, the start now names the directory and
  the `.gitignore` fix instead of only reporting an ambiguous baseline (#15902).
