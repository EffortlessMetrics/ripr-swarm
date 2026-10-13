<!-- section: Fixed -->
- The VS Code extension binds its ripr session to the one selected workspace
  folder. Opening a supported file from another root now rebinds to the
  latest root instead of keeping the earlier session, queued starts wait for
  a root removal to finish, and removing the active root pauses analysis and
  quarantines repair actions until explicit root recovery. Stale context and
  agent-command copies fail closed, while the context copy keeps its CLI
  fallback when the server failed to start. A real-server multi-root matrix
  exercises the developer journey (#1873).
