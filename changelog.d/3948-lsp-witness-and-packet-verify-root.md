<!-- section: Fixed -->
- The LSP finding diagnostic and hover now print the discriminator
  witness `explain_command` bound to the workspace's absolute root
  (with `--worktree`) instead of `--root .`, so pasting it from any
  terminal directory explains the same finding. The normalized
  diagnostic payload digest projects the bound root away, so equal
  findings in relocated checkouts keep equal digests. Standalone
  `ripr agent packet --root <dir>` canonical items embed a verify
  command bound to that root, and its typed command spec is recovered
  against the selected root instead of `.` (#3948, #4001).
