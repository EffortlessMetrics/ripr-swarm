<!-- section: Fixed -->
- MCP: the workspace status document now carries the "ready means only that
  a repository root was discovered — not that analysis ran or that no issues
  were found" limitation in its own `limitations`, so agents consuming
  `structuredContent` or the status resource without the tool description
  cannot misread `workspace_state: ready` as a clean result (#10540).
