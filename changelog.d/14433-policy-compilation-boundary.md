<!-- section: Internal / CI -->
- Repository policy workflow contracts use a separate unpublished test boundary
  and a shared subprocess-safe executable, preserving full test-resource custody
  without rebuilding the normal policy CLI for isolated test-only edits (#14433).
