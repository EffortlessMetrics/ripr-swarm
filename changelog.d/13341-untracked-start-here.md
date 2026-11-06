<!-- section: Fixed -->
- `ripr check`: on a clean base branch whose only new source is untracked, the Start-here Safe next action now names the untracked files and the `git add -N` staging repair (then `ripr check --worktree`, or `--diff PATH`) instead of the tracked-only "edit a tracked source file" wording that contradicted the header (#13341).
