<!-- section: Fixed -->
- `ripr mcp --stdio` stops when it receives SIGTERM or Ctrl-C while idle
  (Unix). The first signal used to be held for the next `ripr_refresh`, so one
  `kill` or Ctrl-C left the server running until stdin closed and a later
  refresh came back cancelled. A signal during a refresh still cancels only
  that refresh and keeps the session (#14102).
