<!-- section: Internal / CI -->
- Queue extension selection compares G with the freshly fetched destination's
  common ancestor, so a speculative prefix's extension change is not hidden by
  an upper docs-only change. Unavailable or ambiguous destination history keeps
  extension proof selected (#14500).
