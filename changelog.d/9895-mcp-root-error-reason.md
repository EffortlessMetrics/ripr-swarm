<!-- section: Fixed -->
- `ripr mcp --stdio --root <path>` startup warning for an unusable root now names
  the specific reason (missing path, not a directory, unresolvable path, or no
  repository marker) and the concrete next step, instead of the circular
  "not a usable repository root", using the same wording as the
  `workspace_unavailable` tool replies (#9895).
