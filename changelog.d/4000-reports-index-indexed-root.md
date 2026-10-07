<!-- section: Fixed -->
- `ripr reports index` regeneration commands now follow the index's inputs.
  `--root <dir>` was recorded but every command printed `--root .`, so a
  pasted command analyzed the current directory, and explicit
  `--reports-dir`/`--review-dir`/... values were ignored in favor of the
  default `target/ripr/...` paths. Commands now name the input directories;
  for a non-default root they name the resolved repository with absolute
  artifact paths, and the Markdown keeps a root with a backtick inside one
  code span. The default packet's output is unchanged (#4000).
