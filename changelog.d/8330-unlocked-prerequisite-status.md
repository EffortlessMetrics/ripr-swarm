<!-- section: Internal / CI -->
- Stop unlocked package qualification when packaging, identity, extraction,
  directory entry, or lockfile preparation fails, preserving the failing command's
  status instead of continuing to a misleading successful check (#8330).
