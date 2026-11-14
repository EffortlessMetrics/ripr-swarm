<!-- section: Internal / CI -->
- Bound each Rayon pool in verdict-corpus analyzer children to one worker while
  retaining parent case parallelism, case-local caches and complete corpus checks (#14586).
