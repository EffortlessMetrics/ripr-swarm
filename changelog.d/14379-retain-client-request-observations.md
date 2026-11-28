<!-- section: Internal / CI -->
- The VS Code multi-root matrix retains client code-action observations in its
  bounded diagnostic trace and restores request observers after each command.
  Cancellation failures remain failures; these observations do not identify a
  cancellation cause (#14379).
