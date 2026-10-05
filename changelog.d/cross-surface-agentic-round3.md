<!-- section: Fixed -->
- `ripr lsp`'s `ripr.collectContext` witness command now replays the
  session's saved-worktree diff source (`--worktree` with the session
  root), so running the packet's own command re-selects the finding it
  ships with instead of exiting 2 against the committed default-branch
  diff (#5994).
- `ripr mcp` `ripr_refresh` and `ripr://workspace/status` disclose the
  refresh's committed default-branch scope whenever tracked files carry
  uncommitted edits, naming `ripr check --worktree` as the route that
  analyzes them; a dirty-tree `no_scope` no longer reads as all-clear
  (#5995).
- One finding's file location renders as one workspace-relative string
  on check, explain, context, LSP and MCP through the shared
  `analysis::finding_location_text` owner; MCP no longer leaks the
  verbatim `//?/` path form, and absolute-root runs join across
  surfaces by file string (#5996).
