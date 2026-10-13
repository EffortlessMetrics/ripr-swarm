<!-- section: Fixed -->
- `ripr check --base HEAD~99` no longer tells the user to add or fetch a
  remote. `HEAD~` is the same walk as `HEAD~1`. The refusal counts
  first-parent commits, so a merged side branch is not treated as a step
  `~` can take. `HEAD^2` on a non-merge commit says that parent does not
  exist. Shallow clones still say `git fetch --unshallow`, and
  remote-qualified refs such as `origin/main` still name their remote
  (#7653).
