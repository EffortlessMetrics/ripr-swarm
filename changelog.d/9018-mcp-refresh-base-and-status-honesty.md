<!-- section: Fixed -->
- `ripr_refresh` takes an optional `base` revision, so a repository whose default
  branch is not `main`/`master` (for example `trunk`) has a recovery route that
  works from inside MCP; the failure recovery now names it instead of a CLI
  `--base` flag the MCP session cannot receive. `ripr_workspace_status` adds
  `root.git_work_tree` (`not_detected` or `marker_invalid`, with a limitation)
  for a validated root without a usable Git work tree, where refresh can never
  succeed, and names an explicit base; base-less callers see unchanged v1 shapes.
  `attempt_not_found` no longer implies a durable store ever held a
  `ripr_prepare_repair` session transaction; the refresh docs state the
  `target/ripr/` cache location (#9018).
