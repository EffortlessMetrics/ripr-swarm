<!-- section: Fixed -->
- Every paste-able command the agent brief renders — the `next` block's
  `inspect_packet` and `verify_after_edit` and each top seam's
  `before_snapshot_command`, `after_snapshot_command`, and
  `verify_command` — now renders the workspace root with
  `root_path_display` before `bound_root`, so a Unix checkout whose path
  contains a literal backslash (e.g. `/tmp/team\repo`) gets a paste-ready
  command that targets the user's checkout instead of a backslash-rewritten
  sibling directory. Ordinary roots and all Windows rendering are unchanged
  (#8995, #4287).
