<!-- section: Internal / CI -->
- The VS Code extension's process-group timeout test now treats a zombie
  descendant as terminated, so it no longer fails in containers whose PID 1
  does not reap orphans; a descendant that is still running still fails it
  (#10256).
