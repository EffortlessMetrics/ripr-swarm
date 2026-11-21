<!-- section: Fixed -->
- The pre-commit hook no longer blocks a new project's first commit: `ripr check --worktree --base HEAD` on an unborn `HEAD` diffs the staged content against the empty tree instead of exiting 2, and an oversized diff's refusal now names `SKIP=ripr git commit ...` and the hook `args:` for `--path`/`--exclude` (#15398).
