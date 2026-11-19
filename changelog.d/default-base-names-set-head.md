<!-- section: Fixed -->
- `ripr check` with no resolvable default base now names `git remote set-head origin --auto` when `origin` exists but has no `origin/HEAD` (a `git remote add` + `git fetch` repository), so a pre-commit hook that cannot pass `--base` has a literal fix (#TBD).
