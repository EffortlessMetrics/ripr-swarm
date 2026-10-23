<!-- section: Fixed -->
- `ripr mcp --stdio` no longer silently re-serves a full result to a second
  well-formed `initialize` on an initialized session. It now answers with
  `-32600` (invalid request) saying the session is already initialized, and
  keeps serving (#9895).
