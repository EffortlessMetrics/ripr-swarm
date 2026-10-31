<!-- section: Fixed -->
- TypeScript committed-snapshot credit now reads a `.snap` file only when the
  repository tracks it, so in a `--worktree` run an untracked or ignored
  snapshot file (a local file CI never sees) stays `snapshot` / medium. When
  the repository cannot list its snapshot files the file is read as before.
  The runner-config walk and the tracked-file list are also memoized per
  analysis run, so each ancestor directory is read once instead of once per
  test file (#7708).
