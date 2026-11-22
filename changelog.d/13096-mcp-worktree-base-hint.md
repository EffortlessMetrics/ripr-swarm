<!-- section: Fixed -->
- MCP `ripr_refresh` now names the way out when a worktree snapshot changed files but held no behavior to check, which is the usual result of editing only tests after committing the production change onto the base. The new `worktree_base_hint` says to retry with `base` set to a commit before that change, matching the hint `ripr check` already prints (#13096).
