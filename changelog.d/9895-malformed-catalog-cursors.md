<!-- section: Fixed -->
- MCP static catalog lists reject malformed non-null cursors with the same
  structured omission recovery as string cursors, while preserving protocol
  validation, session recovery, null defaults and explicit gap paging (#9895).
