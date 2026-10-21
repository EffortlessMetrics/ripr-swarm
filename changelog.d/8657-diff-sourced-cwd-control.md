<!-- section: Internal / CI -->
- The honesty-scope control for pinned-external `--diff` paths now also pins the
  `.patch` extension and a missing plain directory, which read no filesystem
  state and so cannot depend on process cwd (#8657).
