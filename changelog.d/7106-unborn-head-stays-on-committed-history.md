<!-- section: Fixed -->
- A repository whose `HEAD` is unborn or dangling is no longer read as a
  dirty working tree by `ripr check`'s default source selection. Its staged
  files show as new additions, which used to start a working-tree run that
  completed against nothing. The run now stays on committed history, so the
  diff loader refuses with the `git rev-parse HEAD` repair. Explicit
  `--worktree` is unchanged (#7106).
