<!-- section: Fixed -->
- Unix MCP refreshes settle SIGINT and SIGTERM with a typed failure, clean up
  their owned child groups before reaping, and allow subsequent refreshes
  on the same session. Repeated signals retain the native exit escape (#9979).
