<!-- section: Fixed -->
- `ripr mcp --stdio --root <path>` startup warning for an unusable root now names
  the specific reason (missing path, not a directory, unresolvable path, or no
  repository marker) instead of the circular "not a usable repository root".
  It shares the error code and cause text with the `workspace_unavailable` tool
  replies, and adds a concrete next step only in the startup warning (#9895).
