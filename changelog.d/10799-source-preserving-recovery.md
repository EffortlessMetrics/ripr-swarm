<!-- section: Fixed -->
- MCP gap-overflow and failed-refresh recovery retain the captured or
  requested input source instead of silently switching saved worktree edits
  to committed history. Gap recovery also retains its repository and base,
  states the bounded current-state rerun limitation, and refuses overly long
  complete routes instead of truncating commands. Discovery and help separate
  the committed default from opt-in saved tracked edits (#10799, #7357).
