<!-- section: Changed -->
- The LSP diagnostics refresh no longer runs the worktree disclosure-probe
  worker (untracked-routed-files naming, analyzed-revisions naming): it never
  rendered those fields, and on a loaded host the probe join extended every
  didOpen-to-first-diagnostics refresh by the probes' full wall time. CLI and
  MCP check output keep both disclosures, and they now reuse the diff
  loader's already-resolved base commit and merge-base origin instead of
  spawning identical git probes again (#7716, #7757).
