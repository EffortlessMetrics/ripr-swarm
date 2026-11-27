<!-- section: Fixed -->
- `ripr check` with no resolvable default base now names `git fetch origin` followed by `git remote set-head origin --auto` when `origin` exists but has no `origin/HEAD` (a repository set up with `git remote add`), so a pre-commit hook that cannot pass `--base` has a literal fix (#15380).
