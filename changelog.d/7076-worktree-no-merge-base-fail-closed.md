<!-- section: Fixed -->
- A working-tree read (`--worktree`, or the dirty-tree default) with no merge
  base fails closed instead of diffing from the base tip: `git diff
  <base-tip>` exits 0 and reports the base tip's content reversed as local
  changes, so a shallow clone, an orphan branch, or an unborn HEAD could exit
  0 with findings on code the user never touched. The run now refuses with
  the committed path's cause and repair (the shallow-clone unshallow route,
  the unrelated-histories `--base` route, or the unborn-HEAD route) and an
  `analysis_failed` JSON refusal with no findings (#7076).
