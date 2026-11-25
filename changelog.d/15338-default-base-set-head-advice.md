<!-- section: Fixed -->
- `ripr init`'s closing line no longer promises a default-branch comparison when
  `ripr check` would refuse for want of a default base (default branch not
  `main`/`master`, `origin/HEAD` unset); it names `git remote set-head origin --auto`
  and `ripr check --base <ref>`. A bad explicit `--base` is no longer answered
  with "pass `--base <ref>`" (#15338).
