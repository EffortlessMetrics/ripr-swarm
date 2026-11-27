<!-- section: Fixed -->
- `ripr check` on a clean checkout whose base resolves to `origin/<same branch>` at the same commit now says you are on the comparison base instead of suggesting `--worktree`, for `master` and for any other branch name (such as `trunk`) when no other base candidate exists (#15326).
