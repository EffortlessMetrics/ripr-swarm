<!-- section: Added -->
- `.pre-commit-hooks.yaml` adds a `ripr-push` hook (`ripr check --committed
  --quiet` at push time). The `ripr` hook now runs only at commit time, so a
  push-time install no longer prints "nothing was in scope" on every push.
  `docs/INTEGRATIONS.md` adds lefthook, husky and plain Git hook snippets
  (#14348).
