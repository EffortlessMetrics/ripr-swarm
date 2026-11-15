<!-- section: Fixed -->
- `ripr -v check` no longer prints the diff scope as `ripr: mode = Worktree`.
  The line now reads `ripr: mode = draft; scope = worktree`: the `--mode`
  value the report header prints, and the scope under its own name with the
  token the progress lines use (#14093).
