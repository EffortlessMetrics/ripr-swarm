<!-- section: Fixed -->
- `ripr mcp --stdio` answers a second `initialize` on an initialized session
  with `-32600` (invalid request) saying the session is already initialized,
  instead of a bare method-not-found, and keeps serving (#9895).
