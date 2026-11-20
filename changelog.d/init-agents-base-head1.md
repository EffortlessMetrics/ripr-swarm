<!-- section: Changed -->
- The `ripr init --agents` block now tells an agent that committed on the default
  branch itself to pass `--base HEAD~N` (N = its commit count), and says "no default
  branch" instead of naming only `origin/main|master`. In the 2026-11-15 A/B runs,
  47% of agent sessions spent their first `ripr check` on a `no_scope` result for
  this case before retrying (#12912).
