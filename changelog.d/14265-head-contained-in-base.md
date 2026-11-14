<!-- section: Changed -->
- `ripr check` on a branch that is behind its base, or already merged into it, now says HEAD is already contained in the comparison base and that the diff is empty, and its next action names the branch switch or `ripr check --base HEAD~1`, instead of blaming analyzed languages and suggesting a commit or `--worktree` (#14265).
