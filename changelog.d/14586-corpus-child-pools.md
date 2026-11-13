<!-- section: Internal / CI -->
- Bound each verdict-corpus analyzer child's Rayon pool to one worker while
  retaining parent case parallelism, case-local caches and complete corpus checks (#14586).
