<!-- section: Fixed -->
- `ripr lsp --stdio` no longer waits for the client to answer
  `window/workDoneProgress/create` before starting an analysis refresh: the
  progress announcement is sent detached, and a refresh that finishes first
  never gets a stale `Begin` (#9149).
