<!-- section: Changed -->
- The `ripr init --agents` block now tells an agent to add `--base HEAD~1` when it
  committed on the base branch of a repository with no `origin/main|master`. In the
  2026-11-15 A/B runs, 47% of agent sessions spent their first `ripr check` on a
  `no_scope` result for exactly this case before retrying (#12912).
