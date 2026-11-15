<!-- section: Added -->
- The repository root now ships `.pre-commit-hooks.yaml` with a `ripr` hook
  (`language: system`, advisory `ripr check --worktree --base HEAD --quiet`), so
  projects can add `repo: https://github.com/EffortlessMetrics/ripr` to
  `.pre-commit-config.yaml` instead of hand-writing a `repo: local` hook;
  `docs/INTEGRATIONS.md` shows both forms (#14337).
