<!-- section: Fixed -->
- The VS Code extension no longer leaves a permanent "ripr: queued" status
  after an unsaved buffer is closed without saving; the status bar restores
  the last completed saved-workspace analysis, or reports the paused state
  with the real refresh triggers before the first completed refresh (#7340).
