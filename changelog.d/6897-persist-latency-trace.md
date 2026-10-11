<!-- section: Changed -->
- `ripr agent repair`: setting `RIPR_PERSIST_LATENCY_TRACE` emits
  diagnostic `ripr_persist_latency` wall-clock lines for baseline
  capture (git inventory, worktree identity, stability recheck,
  serialize, write), per-artifact staging (read, write, digest), and
  the persist total. Unset, stdout and stderr are unchanged. The
  repair help names the switch (#6897).
