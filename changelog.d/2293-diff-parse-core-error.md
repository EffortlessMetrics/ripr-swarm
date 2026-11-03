<!-- section: Internal / CI -->
- Bounded unified-diff parse env and metadata failures now travel through the
  shared `CoreError` boundary with String compatibility wrappers retained for
  legacy callers (`Advances #2293`).
